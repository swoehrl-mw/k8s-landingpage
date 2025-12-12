use serde::Deserialize;

#[derive(Deserialize, Debug, Clone, Default)]
pub struct Config {
    pub global: Option<Global>,
    pub local: Option<LocalCluster>,
    pub remote: Option<Vec<RemoteCluster>>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Global {
    #[serde(default)]
    pub only_with_annotation: bool,
    pub refresh_interval_seconds: Option<u64>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct LocalCluster {
    pub enabled: bool,
    pub description: Option<String>,
    pub namespaces: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCluster {
    pub name: String,
    pub description: Option<String>,
    pub kubeconfig_secret: KubeconfigSecret,
    pub namespaces: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Clone, Default)]
pub struct KubeconfigSecret {
    pub name: String,
    pub namespace: String,
}

pub fn read_config() -> Config {
    let data = std::fs::read_to_string(
        std::env::var("CONFIG_FILE").unwrap_or_else(|_| "config.yaml".to_owned()),
    )
    .unwrap();
    serde_yaml::from_str(&data).unwrap()
}
