impl<Cx: ClientContext, T: Transport> ApiClient<Cx, T> {
    fn build_request<E, F>(
        &self,
        ep: &E,
        meta: RequestMeta,
        patch_policy: &F,
    ) -> Result<BuiltRequest, ApiClientError>
    where
        E: Endpoint<Cx>,
        F: for<'a> Fn(&mut PolicyPatch<'a>) -> Result<(), ApiClientError>,
    {
        let ctx = Self::ctx_for::<E>(ep);
        // Route = base + endpoint route part
        let mut route = Cx::base_route(self.vars(), self.auth_vars());
        <E::Route as RoutePart<Cx, E>>::apply(ep, self.vars(), self.auth_vars(), &mut route)?;

        // Policy layering model:
        // client (base_policy) -> (prefix/path) -> endpoint -> runtime injections
        let mut policy = Cx::base_policy(self.vars(), self.auth_vars(), &ctx)?;
        policy.set_layer(PolicyLayer::Endpoint);
        <E::Policy as PolicyPart<Cx, E>>::apply(ep, self.vars(), self.auth_vars(), &mut policy)?;

        // Runtime Accept injection (decoder-owned) after endpoint policy.
        policy.set_layer(PolicyLayer::Runtime);
        let is_head = E::METHOD == http::Method::HEAD;
        if !is_head && !E::response_is_no_content() {
            policy.ensure_accept(E::accept_content_type());
        }

        // Runtime patch (pagination controller, etc.)
        {
            let mut patch = PolicyPatch::new(ctx.clone(), &mut policy);
            patch_policy(&mut patch)?;
        }

        // Compute parts after patch (Content-Type may have been added/removed there).
        let (mut headers, query, timeout, cache, retry, mut rate_limit) = policy.into_parts();
        rate_limit.canonicalize();

        // URL
        route.host().validate(ctx.clone())?;
        let host = route.host().join(Cx::DOMAIN);
        let base = format!("{}://{}", Cx::SCHEME, host);
        let mut url = url::Url::parse(&base).map_err(|e| ApiClientError::BuildUrl {
            ctx: ctx.clone(),
            source: e,
        })?;
        url.set_path(route.path().as_str());
        {
            let mut qp = url.query_pairs_mut();
            for (k, v) in query.iter() {
                qp.append_pair(k, v);
            }
        }

        // Body (optional) + Content-Type injection if missing.
        let mut body_bytes: Option<Bytes> = None;
        if let Some(body) = <E::Body as BodyPart<E>>::body(ep) {
            let encoded = <<E::Body as BodyPart<E>>::Enc as Encodes<
                <E::Body as BodyPart<E>>::Body,
            >>::encode(body)
            .map_err(|e| ApiClientError::codec_error(ctx.clone(), e))?;

            if !headers.contains_key(CONTENT_TYPE) {
                let ct = <<E::Body as BodyPart<E>>::Enc as ContentType>::CONTENT_TYPE;
                if !ct.is_empty() {
                    headers.insert(CONTENT_TYPE, http::HeaderValue::from_static(ct));
                }
            }
            body_bytes = Some(encoded);
        }

        Ok(BuiltRequest {
            meta,
            url,
            headers,
            body: body_bytes,
            timeout,
            cache,
            cache_mode: CacheRequestMode::Default,
            retry,
            rate_limit,
            cache_revalidation: None,
            extensions: Default::default(),
        })
    }

}
