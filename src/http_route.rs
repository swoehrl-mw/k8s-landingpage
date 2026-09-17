// Generated with kopium 0.24.1 from the Gateway API v1.6.2 HTTPRoute CRD.
// Only fields read by the collector are retained; serde ignores all other fields.

use kube::CustomResource;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Serialize, Deserialize, Clone, Debug)]
#[kube(
    group = "gateway.networking.k8s.io",
    version = "v1",
    kind = "HTTPRoute",
    root = "HttpRoute",
    plural = "httproutes"
)]
#[kube(namespaced)]
#[kube(schema = "disabled")]
pub struct HttpRouteSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostnames: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<Vec<HttpRouteRules>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HttpRouteRules {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matches: Option<Vec<HttpRouteRulesMatches>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HttpRouteRulesMatches {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<HttpRouteRulesMatchesPath>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HttpRouteRulesMatchesPath {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::HttpRoute;

    #[test]
    fn deserializes_collector_fields() {
        let route: HttpRoute = serde_json::from_value(serde_json::json!({
            "apiVersion": "gateway.networking.k8s.io/v1",
            "kind": "HTTPRoute",
            "metadata": { "name": "example" },
            "spec": {
                "hostnames": ["example.com"],
                "parentRefs": [{ "name": "gateway" }],
                "rules": [{
                    "backendRefs": [{ "name": "service", "port": 80 }],
                    "matches": [{ "path": { "type": "PathPrefix", "value": "/app" } }]
                }]
            }
        }))
        .unwrap();

        assert_eq!(route.spec.hostnames.unwrap(), ["example.com"]);
        assert_eq!(
            route.spec.rules.unwrap()[0].matches.as_ref().unwrap()[0]
                .path
                .as_ref()
                .unwrap()
                .value
                .as_deref(),
            Some("/app")
        );
    }
}
