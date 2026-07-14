// Client lifecycle phase modules intentionally share one private parent namespace.
use super::*;

pub(super) struct PublicRequestHead {
    pub(super) meta: RequestExecutionMeta,
    pub(super) url: url::Url,
    pub(super) headers: http::HeaderMap,
    pub(super) timeout: Option<std::time::Duration>,
    pub(super) rate_limit: RateLimitPlan,
    pub(super) auth_plan: crate::auth::AuthPlacementPlan,
    pub(super) reserved_headers: Vec<http::HeaderName>,
}

impl PublicRequestHead {
    pub(super) fn apply_auth_preflight(
        &mut self,
        auth_plan: &crate::auth::AuthPlacementPlan,
        ctx: &ErrorContext,
    ) -> Result<(), ApiClientError> {
        auth_plan
            .validate_public_request_with_reserved_headers(
                &self.headers,
                &self.url,
                &self.reserved_headers,
            )
            .map_err(|source| ApiClientError::Auth {
                ctx: ctx.clone(),
                source,
            })?;
        self.auth_plan = auth_plan.clone();
        Ok(())
    }

    pub(super) fn finish(
        self,
        client: &reqwest::Client,
        body: crate::io::ProducedBody,
        ctx: &ErrorContext,
    ) -> Result<BuiltRequest, ApiClientError> {
        let mut headers = self.headers;
        let builder = client.request(self.meta.method.clone(), self.url.clone());
        let (builder, terminal_media_type, body_errors) =
            body.apply_to_reqwest(builder)
                .map_err(|_| ApiClientError::PolicyViolation {
                    ctx: ctx.clone(),
                    msg: "native request body materialization failed",
                })?;
        crate::io::apply_execution_media_type(&mut headers, terminal_media_type.as_ref()).map_err(
            |()| ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "request Content-Type conflicts with produced body media type",
            },
        )?;
        let mut builder = builder.headers(headers);
        if let Some(timeout) = self.timeout {
            builder = builder.timeout(timeout);
        }
        let message = builder
            .build()
            .map_err(|_| ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "native request construction failed",
            })?;
        let logical_url = message.url().clone();
        #[cfg(feature = "dangerous-dev-tools")]
        let auth_query_keys = self.auth_plan.sensitive_query_keys.clone();
        #[cfg(feature = "dangerous-dev-tools")]
        let protected_header_names = self
            .auth_plan
            .slots
            .iter()
            .filter_map(|slot| match &slot.placement {
                crate::auth::PlannedAuthPlacement::Bearer
                | crate::auth::PlannedAuthPlacement::Basic => Some(http::header::AUTHORIZATION),
                crate::auth::PlannedAuthPlacement::Header(name) => Some(name.clone()),
                crate::auth::PlannedAuthPlacement::Query(_) => None,
            })
            .collect();
        let context = crate::transport::RequestExecutionContext {
            meta: self.meta,
            logical_url,
            timeout: self.timeout,
            body_errors,
            #[cfg(feature = "dangerous-dev-tools")]
            auth_query_keys,
            #[cfg(feature = "dangerous-dev-tools")]
            protected_header_names,
        };
        Ok(BuiltRequest {
            message,
            context,
            auth_plan: self.auth_plan,
            rate_limit: self.rate_limit,
        })
    }
}

impl<Cx: ClientContext> ApiClient<Cx> {
    pub(super) fn resolve_public_request_head(
        &self,
        plan: &crate::endpoint::RequestPlanView,
        body: &crate::io::PreparedBody,
        meta: RequestExecutionMeta,
    ) -> Result<PublicRequestHead, ApiClientError> {
        let ctx = ErrorContext {
            endpoint: plan.endpoint.meta.name,
            method: plan.endpoint.meta.method.clone(),
        };
        let timeout = plan.overrides.timeout.or(plan.endpoint.policy.timeout);
        let rate_limit = plan.endpoint.policy.rate_limit.clone();

        let base = format!(
            "{}://{}",
            plan.endpoint.route.scheme, plan.endpoint.route.host
        );
        let mut url = url::Url::parse(&base).map_err(|e| ApiClientError::BuildUrl {
            ctx: ctx.clone(),
            source: e,
        })?;
        url.set_path(&plan.endpoint.route.path);
        if !plan.endpoint.policy.query.is_empty() {
            let mut qp = url.query_pairs_mut();
            for (k, v) in &plan.endpoint.policy.query {
                qp.append_pair(k, v);
            }
        }

        let public_header_error = |source: crate::header_ownership::HeaderOwnershipError,
                                   scope: &'static str| {
            let scope = match scope {
                "client-wide" => "credential-bearing client-wide header",
                _ => "credential-bearing endpoint header",
            };
            if crate::redaction::is_credential_bearing_header_name(source.header_name().as_str()) {
                ApiClientError::Auth {
                    ctx: ctx.clone(),
                    source: crate::auth::AuthError::new(
                        crate::auth::AuthErrorKind::InvalidConfiguration,
                        format!("{scope} collides with authentication ownership"),
                    ),
                }
            } else {
                ApiClientError::PolicyViolation {
                    ctx: ctx.clone(),
                    msg: "public API headers include a transport-reserved header",
                }
            }
        };
        crate::header_ownership::validate_public_headers(&self.api_headers)
            .map_err(|source| public_header_error(source, "client-wide"))?;
        crate::header_ownership::validate_public_headers(&plan.endpoint.policy.headers)
            .map_err(|source| public_header_error(source, "endpoint"))?;
        // Resolve client-wide runtime headers first; endpoint headers retain
        // the established replacement semantics.
        let mut headers = self.api_headers.clone();
        for name in plan.endpoint.policy.headers.keys() {
            headers.remove(name);
        }
        for (name, value) in &plan.endpoint.policy.headers {
            headers.append(name.clone(), value.clone());
        }
        // These are origin-facing headers, deliberately not Reqwest defaults.
        headers
            .entry(http::header::ACCEPT)
            .or_insert_with(|| http::HeaderValue::from_static("*/*"));
        headers.insert(
            http::header::USER_AGENT,
            http::HeaderValue::from_static(concat!("concord/", env!("CARGO_PKG_VERSION"))),
        );
        crate::io::apply_prepared_body_media_type(&mut headers, body).map_err(|()| {
            ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg: "request Content-Type conflicts with prepared body media type",
            }
        })?;
        let mut reserved_headers = Vec::new();
        if body.reserves_content_type() || body.media_type().is_some() {
            reserved_headers.push(http::header::CONTENT_TYPE);
        }
        Ok(PublicRequestHead {
            meta,
            url,
            headers,
            timeout,
            rate_limit,
            auth_plan: crate::auth::AuthPlacementPlan::default(),
            reserved_headers,
        })
    }

    pub(super) fn produce_execution_body(
        &self,
        body: &mut crate::io::PreparedBody,
        ctx: &ErrorContext,
    ) -> Result<crate::io::ProducedBody, ApiClientError> {
        body.produce_for_execution().map_err(|error| {
            if let Some(source) = error.body_error() {
                return ApiClientError::request_body_production(ctx.clone(), source);
            }
            let msg = match error.kind() {
                crate::io::BodyProductionErrorKind::AlreadyConsumed => {
                    "one-shot request body has already been consumed"
                }
                crate::io::BodyProductionErrorKind::FactoryFailure => "request body factory failed",
            };
            ApiClientError::PolicyViolation {
                ctx: ctx.clone(),
                msg,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BuildCx;

    impl ClientContext for BuildCx {
        type Vars = ();
        type AuthVars = ();
        type AuthState = ();
        const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
        const DOMAIN: &'static str = "example.test";

        fn init_auth_state(_: &Self::Vars, _: &Self::AuthVars) -> Self::AuthState {}
    }

    fn header_test_plan(headers: http::HeaderMap) -> crate::endpoint::RequestPlanView {
        crate::endpoint::RequestPlanView {
            endpoint: crate::endpoint::EndpointPlan {
                meta: crate::endpoint::EndpointMeta {
                    name: "HeaderOwnership",
                    method: http::Method::GET,
                    idempotent: true,
                    facade_path: &[],
                },
                route: crate::endpoint::ResolvedRoute::new(
                    http::uri::Scheme::HTTPS,
                    "example.test",
                    "/headers",
                ),
                policy: crate::policy::ResolvedPolicy {
                    headers,
                    ..Default::default()
                },
                response: crate::endpoint::ResponsePlan {
                    accept: None,
                    no_content: false,
                    format: crate::codec::Format::Text,
                },
                pagination: None,
            },
            overrides: crate::endpoint::RequestOverrides::default(),
        }
    }

    #[test]
    fn client_headers_merge_before_endpoint_headers_and_concord_materializes_defaults() {
        let mut client_headers = http::HeaderMap::new();
        client_headers.insert("x-shared", http::HeaderValue::from_static("client"));
        client_headers.insert("x-client", http::HeaderValue::from_static("present"));
        let client = ApiClient::<BuildCx>::new((), ())
            .with_api_headers(client_headers)
            .expect("safe client headers");

        let mut endpoint_headers = http::HeaderMap::new();
        endpoint_headers.append("x-shared", http::HeaderValue::from_static("endpoint-a"));
        endpoint_headers.append("x-shared", http::HeaderValue::from_static("endpoint-b"));
        let head = client
            .resolve_public_request_head(
                &header_test_plan(endpoint_headers),
                &crate::io::PreparedBody::empty(),
                RequestExecutionMeta {
                    endpoint: "HeaderOwnership",
                    method: http::Method::GET,
                    idempotent: true,
                    page_index: 0,
                },
            )
            .expect("header preparation");

        assert_eq!(
            head.headers.get("x-client"),
            Some(&http::HeaderValue::from_static("present"))
        );
        let shared = head.headers.get_all("x-shared");
        assert_eq!(shared.iter().count(), 2);
        assert_eq!(
            head.headers.get(http::header::ACCEPT),
            Some(&http::HeaderValue::from_static("*/*"))
        );
        assert_eq!(
            head.headers.get(http::header::USER_AGENT),
            Some(&http::HeaderValue::from_static(concat!(
                "concord/",
                env!("CARGO_PKG_VERSION")
            )))
        );
    }

    #[test]
    fn protocol_headers_are_rejected_at_client_and_endpoint_boundaries() {
        let mut forbidden = http::HeaderMap::new();
        forbidden.insert(
            http::header::HOST,
            http::HeaderValue::from_static("elsewhere.test"),
        );
        assert!(
            ApiClient::<BuildCx>::new((), ())
                .with_api_headers(forbidden)
                .is_err()
        );

        let mut endpoint_headers = http::HeaderMap::new();
        endpoint_headers.insert(
            http::header::ACCEPT_ENCODING,
            http::HeaderValue::from_static("gzip"),
        );
        let result = ApiClient::<BuildCx>::new((), ()).resolve_public_request_head(
            &header_test_plan(endpoint_headers),
            &crate::io::PreparedBody::empty(),
            RequestExecutionMeta {
                endpoint: "HeaderOwnership",
                method: http::Method::GET,
                idempotent: true,
                page_index: 0,
            },
        );
        let Err(error) = result else {
            panic!("transport-owned endpoint header must fail");
        };
        assert!(matches!(error, ApiClientError::PolicyViolation { .. }));
    }

    #[test]
    fn credential_bearing_endpoint_headers_fail_before_authentication_preflight() {
        let mut endpoint_headers = http::HeaderMap::new();
        endpoint_headers.insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("Bearer public-secret"),
        );
        let result = ApiClient::<BuildCx>::new((), ()).resolve_public_request_head(
            &header_test_plan(endpoint_headers),
            &crate::io::PreparedBody::empty(),
            RequestExecutionMeta {
                endpoint: "HeaderOwnership",
                method: http::Method::GET,
                idempotent: true,
                page_index: 0,
            },
        );
        assert!(matches!(result, Err(ApiClientError::Auth { .. })));
    }

    #[test]
    fn client_api_headers_and_endpoint_headers_use_correct_scope_in_public_header_errors() {
        let endpoint_error = ApiClient::<BuildCx>::new((), ()).resolve_public_request_head(
            &header_test_plan({
                let mut headers = http::HeaderMap::new();
                headers.insert(
                    http::header::AUTHORIZATION,
                    http::HeaderValue::from_static("endpoint"),
                );
                headers
            }),
            &crate::io::PreparedBody::empty(),
            RequestExecutionMeta {
                endpoint: "EndpointScope",
                method: http::Method::GET,
                idempotent: true,
                page_index: 0,
            },
        );
        let err = match endpoint_error {
            Ok(_) => panic!("endpoint forbidden header"),
            Err(err) => err,
        };
        let rendered = format!("{err}");
        assert!(rendered.contains("endpoint header"));
        assert!(!rendered.contains("authorization"));

        let client = ApiClient::<BuildCx>::new((), ());
        let mut client = client;
        client.api_headers.insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("public"),
        );
        let client_error = client.resolve_public_request_head(
            &header_test_plan(http::HeaderMap::new()),
            &crate::io::PreparedBody::empty(),
            RequestExecutionMeta {
                endpoint: "ClientScope",
                method: http::Method::GET,
                idempotent: true,
                page_index: 0,
            },
        );
        let err = match client_error {
            Ok(_) => panic!("client wide forbidden header"),
            Err(err) => err,
        };
        let rendered = format!("{err}");
        assert!(rendered.contains("client-wide header"));
        assert!(!rendered.contains("authorization"));
    }

    #[test]
    fn set_api_headers_rejects_credential_bearing_names_without_revealing_raw_name() {
        let mut client = ApiClient::<BuildCx>::new((), ());
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::HeaderName::from_static("x-api-key"),
            http::HeaderValue::from_static("public"),
        );
        let err = client
            .set_api_headers(headers)
            .expect_err("credential-bearing header");
        let rendered = format!("{err}");
        let debug = format!("{err:?}");
        assert!(!rendered.contains("x-api-key"));
        assert!(!debug.contains("x-api-key"));
        assert!(rendered.contains("credential"));
    }

    #[cfg(feature = "default-tls")]
    #[test]
    fn with_safe_reqwest_builder_fallible_rejects_invalid_pem_without_leaking_sentinels() {
        let marker = "BUILD_PEM_SENTINEL";
        let pem = format!(
            "-----BEGIN CERTIFICATE-----\n{marker}\nnot-base64-content\n-----END CERTIFICATE-----"
        );
        let result =
            ApiClient::<BuildCx>::with_safe_reqwest_builder_fallible((), (), move |builder| {
                builder.add_trusted_root_pem(pem.as_bytes())
            });
        let error = match result {
            Ok(_) => panic!("invalid cert must fail"),
            Err(error) => error,
        };
        let diagnostics = format!("{error}\n{:?}", error);
        assert!(!diagnostics.contains(marker), "{diagnostics}");
    }

    #[test]
    fn with_safe_reqwest_builder_fallible_is_accessible_with_infallible_configuration() {
        let _client = ApiClient::<BuildCx>::with_safe_reqwest_builder_fallible((), (), |builder| {
            Ok(builder.connect_timeout(std::time::Duration::from_millis(50)))
        })
        .expect("infallible reqwest configuration should remain ergonomic");
    }

    #[test]
    fn collision_preflight_leaves_one_shot_body_unconsumed() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("public"),
        );
        let mut head = PublicRequestHead {
            meta: RequestExecutionMeta {
                endpoint: "PreflightOneShot",
                method: http::Method::POST,
                idempotent: false,
                page_index: 0,
            },
            url: "https://example.com/items".parse().expect("url"),
            headers,
            timeout: None,
            rate_limit: RateLimitPlan::new(),
            auth_plan: Default::default(),
            reserved_headers: Vec::new(),
        };
        let auth_plan = crate::auth::AuthPlacementPlan::from_auth_plan(&crate::auth::AuthPlan {
            requirements: vec![crate::auth::AuthRequirement {
                credential: crate::auth::CredentialRef {
                    id: crate::auth::CredentialId::new("test", "token"),
                },
                placement: crate::auth::AuthPlacement::Bearer,
                usage_id: crate::auth::AuthUsageId::new("preflight-use"),
                step_id: Some("preflight"),
                provenance: crate::auth::AuthProvenance::new("preflight"),
                challenge: Default::default(),
            }],
        })
        .expect("placement plan");
        let ctx = ErrorContext {
            endpoint: "PreflightOneShot",
            method: http::Method::POST,
        };
        let mut body = crate::io::PreparedBody::one_shot(crate::body::DynBody::empty(), None);

        head.apply_auth_preflight(&auth_plan, &ctx)
            .expect_err("collision must fail");
        assert!(body.produce_for_execution().is_ok());
    }
}
