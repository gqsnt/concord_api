use crate::codec::{ContentType, Decodes, Encodes};
use crate::endpoint::{BodyPart, Endpoint, PolicyPart, ResponseSpec, RoutePart};
use crate::error::ApiClientError;
use crate::policy::Policy;
use crate::types::RouteParts;
use http::header::CONTENT_TYPE;
use http::uri::Scheme;

pub trait ClientContext: Sized {
    type Vars: Clone + Send + Sync + 'static;
    const SCHEME: Scheme;
    const DOMAIN: &'static str;

    fn base_route(_vars: &Self::Vars) -> RouteParts {
        RouteParts::new()
    }

    fn base_policy(_vars: &Self::Vars) -> Policy {
        Policy::new()
    }
}

#[derive(Clone)]
pub struct ApiClient<Cx: ClientContext> {
    http: reqwest::Client,
    vars: Cx::Vars,
}

impl<Cx: ClientContext> ApiClient<Cx> {
    pub fn new(vars: Cx::Vars) -> Self {
        Self {
            http: reqwest::Client::new(),
            vars,
        }
    }


    #[inline]
    pub fn vars(&self) -> &Cx::Vars {
        &self.vars
    }
    #[inline]
    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }


    pub async fn execute<E>(
        &self,
        ep: E,
    ) -> Result<<E::Response as ResponseSpec>::Output, ApiClientError>
    where
        E: Endpoint<Cx>,
    {
        // Route = base + endpoint route part
        let mut route = Cx::base_route(self.vars());
        <E::Route as RoutePart<Cx, E>>::apply(&ep, self, &mut route)?;

        // Policy = base + endpoint policy part
        let mut policy = Cx::base_policy(&self.vars);
        <E::Policy as PolicyPart<Cx, E>>::apply(&ep, self, &mut policy)?;

        // Accept depuis le decoder réponse
        let is_head = E::METHOD == http::Method::HEAD;
        if !is_head && !E::response_is_no_content() {
            policy.ensure_accept(E::accept_content_type());
        }

        // URL
        let host = route.host.join(Cx::DOMAIN);
        let base = format!("{}://{}", Cx::SCHEME, host);
        let mut url = url::Url::parse(&base)?;
        url.set_path(route.path.as_str());
        {
            let mut qp = url.query_pairs_mut();
            for (k, v) in policy.query.iter() {
                qp.append_pair(k, v);
            }
        }
        let has_content_type = policy.has_content_type();
        let mut req = self
            .http
            .request(E::METHOD, url.as_str())
            .headers(policy.headers);

        // Body (optionnel) depuis BodyPart
        if let Some(body) = <E::Body as BodyPart<E>>::body(&ep) {
            let encoded = <<E::Body as BodyPart<E>>::Enc as Encodes<
                <E::Body as BodyPart<E>>::Body,
            >>::encode(body)
            .map_err(ApiClientError::codec_error)?;

            if !has_content_type {
                let ct = <<E::Body as BodyPart<E>>::Enc as ContentType>::CONTENT_TYPE;
                if !ct.is_empty() {
                    req = req.header(CONTENT_TYPE, http::HeaderValue::from_static(ct));
                }
            }

            req = req.body(encoded);
        }

        // Send
        let resp = req.send().await?;
        let resp_headers = resp.headers().clone();
        let status = resp.status();
        let bytes = resp.bytes().await?;

        if !status.is_success() {
            let preview = crate::error::body_as_text(&resp_headers, &bytes);
            return Err(ApiClientError::HttpStatus {
                status,
                headers: resp_headers,
                body: preview,
            });
        }

        // Decode
        let decoded = <<E::Response as ResponseSpec>::Dec as Decodes<
            <E::Response as ResponseSpec>::Decoded,
        >>::decode(&bytes)
        .map_err(|e| ApiClientError::Decode {
            source: e.into(),
            body: crate::error::body_as_text(&resp_headers, &bytes),
        })?;

        let out = <E::Response as ResponseSpec>::map(decoded)?;
        Ok(out)
    }
}
