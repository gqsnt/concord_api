#[derive(Debug, Default)]
#[allow(dead_code)]
pub(crate) struct ResolvedApi {
    pub client: ResolvedClient,
    pub credentials: Vec<ResolvedCredential>,
    pub profiles: Vec<ResolvedProfile>,
    pub defaults: Vec<ResolvedDefault>,
    pub endpoints: Vec<ResolvedEndpoint>,
    pub diagnostics: ResolvedDiagnostics,
}

#[derive(Debug, Default)]
#[allow(dead_code)]
pub(crate) struct ResolvedClient {
    pub name: String,
    pub scheme: String,
    pub host: String,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct ResolvedCredential {
    pub name: String,
    pub kind: String,
    pub redaction_label: String,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct ResolvedProfile {
    pub name: String,
    pub kind: ResolvedProfileKind,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum ResolvedProfileKind {
    Retry,
    RateLimit,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct ResolvedDefault {
    pub owner_path: Vec<String>,
    pub policy: ResolvedPolicy,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct ResolvedEndpoint {
    pub name: String,
    pub facade_path: Vec<String>,
    pub method: String,
    pub route: ResolvedRoute,
    pub params: Vec<ResolvedParam>,
    pub body: Option<ResolvedBody>,
    pub response: ResolvedResponse,
    pub policy: ResolvedPolicy,
    pub diagnostics: ResolvedDiagnostics,
}

#[derive(Debug, Default)]
#[allow(dead_code)]
pub(crate) struct ResolvedRoute {
    pub scheme: String,
    pub host: String,
    pub path_atoms: Vec<ResolvedRouteAtom>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum ResolvedRouteAtom {
    Static(String),
    Param { name: String, encoded: bool },
    Format { source: String },
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct ResolvedParam {
    pub name: String,
    pub ty: String,
    pub required: bool,
    pub default_expr: Option<String>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct ResolvedBody {
    pub arg_name: String,
    pub codec: String,
}

#[derive(Debug, Default)]
#[allow(dead_code)]
pub(crate) struct ResolvedResponse {
    pub codec: String,
    pub output_ty: String,
}

#[derive(Debug, Default)]
#[allow(dead_code)]
pub(crate) struct ResolvedPolicy {
    pub auth_steps: Vec<String>,
    pub headers: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
    pub retry_profile: Option<String>,
    pub rate_limit_profile: Option<String>,
}

#[derive(Debug, Default, Clone)]
#[allow(dead_code)]
pub(crate) struct ResolvedDiagnostics {
    pub provenance: Vec<String>,
    pub notes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_resolved_api_can_hold_one_endpoint() {
        let api = ResolvedApi {
            client: ResolvedClient {
                name: "Api".to_string(),
                scheme: "https".to_string(),
                host: "example.com".to_string(),
            },
            endpoints: vec![ResolvedEndpoint {
                name: "Ping".to_string(),
                facade_path: Vec::new(),
                method: "GET".to_string(),
                route: ResolvedRoute {
                    scheme: "https".to_string(),
                    host: "example.com".to_string(),
                    path_atoms: vec![ResolvedRouteAtom::Static("ping".to_string())],
                },
                params: Vec::new(),
                body: None,
                response: ResolvedResponse {
                    codec: "Text".to_string(),
                    output_ty: "String".to_string(),
                },
                policy: ResolvedPolicy::default(),
                diagnostics: ResolvedDiagnostics::default(),
            }],
            ..ResolvedApi::default()
        };

        assert_eq!(api.endpoints[0].name, "Ping");
        assert_eq!(api.endpoints[0].route.path_atoms.len(), 1);
    }
}
