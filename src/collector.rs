use k8s_openapi::api::{core::v1::Secret, networking::v1::Ingress};
use kube::{
    Api, Client, ResourceExt,
    api::ListParams,
    config::{KubeConfigOptions, Kubeconfig},
};
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::RwLock;

use crate::{
    config::{Config, RemoteCluster},
    errors::{Error, Result},
};

const NAME_ANNOTATION: &str = "landingpage.info/name";
const DESCRIPTION_ANNOTATION: &str = "landingpage.info/description";

#[derive(Clone, Debug, Serialize)]
struct IngressSpec {
    pub name: String,
    pub namespace: String,
    pub host: String,
    pub tls_used: bool,
    pub path: Option<String>,
    pub annotations: BTreeMap<String, String>,
    pub labels: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContextInfo {
    pub clusters: Vec<ClusterInfo>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClusterInfo {
    pub name: String,
    pub description: String,
    pub ingresses: Vec<IngressInfo>,
}

#[derive(Clone, Debug, Serialize)]
pub struct GroupInfo {
    pub name: String,
    pub clusters: Vec<ClusterInfo>,
}

#[derive(Clone, Debug, Serialize)]
pub struct IngressInfo {
    pub name: String,
    pub description: String,
    pub url: String,
}

pub type IngressCollection = Vec<GroupInfo>;
pub type IngressCollectionWrapper = Arc<RwLock<IngressCollection>>;

pub async fn start_collector(config: Config) -> Result<IngressCollectionWrapper> {
    let result = collect_for_all_clusters(&config).await?;
    let info = Arc::new(RwLock::new(result));
    tokio::spawn(run_collector_task(config, info.clone()));
    Ok(info)
}

async fn run_collector_task(config: Config, info: IngressCollectionWrapper) {
    let refresh_interval = config
        .global
        .as_ref()
        .and_then(|g| g.refresh_interval_seconds)
        .unwrap_or(30);
    loop {
        tokio::time::sleep(Duration::from_secs(refresh_interval)).await;
        tracing::info!("Reloading ingresses");
        let new_info = match collect_for_all_clusters(&config).await {
            Ok(result) => result,
            Err(err) => {
                tracing::error!("Encountered error when reloading ingresses: {err}");
                continue;
            }
        };
        let mut lock = info.write().await;
        *lock = new_info;
    }
}

pub async fn collect_for_all_clusters(config: &Config) -> Result<IngressCollection> {
    let mut result = Vec::new();
    let client = kube::Client::try_default().await?;

    // Local cluster as its own group named "local"
    if let Some(local) = config.local.as_ref()
        && local.enabled
    {
        let cluster_info = if let Some(namespaces) = local.namespaces.as_ref() {
            let mut collected = Vec::new();
            for namespace in namespaces.iter() {
                collected
                    .append(&mut collect_ingresses(config, client.clone(), Some(namespace)).await?);
            }
            transform_to_info("local".to_owned(), &local.description, collected)
        } else {
            transform_to_info(
                "local".to_owned(),
                &local.description,
                collect_ingresses(config, client.clone(), None).await?,
            )
        };
        result.push(GroupInfo {
            name: "local".to_owned(),
            clusters: vec![cluster_info],
        });
    }

    // Remote clusters by group
    if let Some(remotes) = config.remote.as_ref() {
        for (group_name, clusters) in remotes.iter() {
            let mut group_clusters = Vec::new();
            for remote in clusters.iter() {
                if let Some(clusterinfo) = collect_from_remote(config, remote, client.clone()).await
                {
                    group_clusters.push(clusterinfo);
                }
            }
            result.push(GroupInfo {
                name: group_name.0.clone(),
                clusters: group_clusters,
            });
        }
    }

    Ok(result)
}

async fn collect_from_remote(
    config: &Config,
    remote: &RemoteCluster,
    client: Client,
) -> Option<ClusterInfo> {
    let remote_client = match kubeconfig(remote, client).await {
        Ok(client) => client,
        Err(err) => {
            tracing::error!("Could not create client to remote cluster: {err}");
            return None;
        }
    };

    if let Some(namespaces) = remote.namespaces.as_ref() {
        let mut collected = Vec::new();
        for namespace in namespaces.iter() {
            match collect_ingresses(config, remote_client.clone(), Some(namespace)).await {
                Ok(mut specs) => collected.append(&mut specs),
                Err(err) => tracing::error!("Could not read ingressess from cluster: {err}"),
            }
        }
        Some(transform_to_info(
            remote.name.clone(),
            &remote.description,
            collected,
        ))
    } else {
        match collect_ingresses(config, remote_client.clone(), None).await {
            Ok(specs) => Some(transform_to_info(
                remote.name.clone(),
                &remote.description,
                specs,
            )),
            Err(err) => {
                tracing::error!("Could not read ingressess from cluster: {err}");
                None
            }
        }
    }
}

async fn kubeconfig(remote: &RemoteCluster, client: Client) -> Result<Client> {
    let secret_api = Api::<Secret>::namespaced(client, &remote.kubeconfig_secret.namespace);
    let error_name = format!(
        "{}/{}",
        remote.kubeconfig_secret.namespace, remote.kubeconfig_secret.name
    );

    let secret = match secret_api.get(&remote.kubeconfig_secret.name).await {
        Ok(result) => result,
        Err(err) => {
            return Err(Error::MissingKubeconfig(format!(
                "Could not get kubeconfig secret {error_name}: {err}"
            )));
        }
    };
    let Some(data) = secret.data.as_ref() else {
        return Err(Error::MissingKubeconfig(format!(
            "Could not get kubeconfig secret {error_name}: No data"
        )));
    };
    let Some(kubeconfig_data) = data.get("value") else {
        return Err(Error::MissingKubeconfig(format!(
            "Could not get kubeconfig secret {error_name}: No data field kubeconfig"
        )));
    };

    let kubeconfig: Kubeconfig = serde_yaml::from_slice(&kubeconfig_data.0)
        .map_err(|err| Error::MissingKubeconfig(err.to_string()))?;
    // create client from kubeconfig
    let mut config =
        kube::Config::from_custom_kubeconfig(kubeconfig, &KubeConfigOptions::default())
            .await
            .map_err(|err| Error::MissingKubeconfig(err.to_string()))?;
    config.accept_invalid_certs = true;
    Ok(config.try_into()?)
}

async fn collect_ingresses(
    config: &Config,
    client: Client,
    namespace: Option<&str>,
) -> Result<Vec<IngressSpec>> {
    let api = if let Some(namespace) = namespace {
        Api::<Ingress>::namespaced(client, namespace)
    } else {
        Api::<Ingress>::all(client)
    };
    let only_with_annotation = config
        .global
        .as_ref()
        .map(|g| g.only_with_annotation)
        .unwrap_or_default();
    let params = ListParams::default();
    let object_list = api.list(&params).await?;

    let mut result = Vec::new();

    for ingress in object_list {
        let name = ingress.name_any();
        if only_with_annotation {
            if let Some(annotations) = ingress.metadata.annotations.as_ref() {
                if annotations.get(NAME_ANNOTATION).is_none()
                    && annotations.get(DESCRIPTION_ANNOTATION).is_none()
                {
                    // none of our annotations, filter it out
                    continue;
                }
            } else {
                // no annotations at all, filter it out
                continue;
            }
        }
        let Some(spec) = ingress.spec else {
            continue;
        };
        for rule in spec.rules.unwrap_or_default() {
            let Some(host) = rule.host else {
                continue;
            };
            for path in rule.http.unwrap_or_default().paths {
                result.push(IngressSpec {
                    name: name.clone(),
                    namespace: ingress
                        .metadata
                        .namespace
                        .clone()
                        .unwrap_or_else(|| "default".to_owned()),
                    host: host.clone(),
                    tls_used: true,
                    path: path.path,
                    annotations: ingress.metadata.annotations.clone().unwrap_or_default(),
                    labels: ingress.metadata.labels.clone().unwrap_or_default(),
                })
            }
        }
    }

    Ok(result)
}

fn transform_to_info(
    cluster_name: String,
    description: &Option<String>,
    input: Vec<IngressSpec>,
) -> ClusterInfo {
    let ingresses = input
        .into_iter()
        .map(|i| {
            let url = format!(
                "https://{}{}",
                i.host,
                i.path.unwrap_or_else(|| "/".to_owned())
            );
            let name = i.annotations.get(NAME_ANNOTATION).unwrap_or(&i.name);
            let description = i
                .annotations
                .get(DESCRIPTION_ANNOTATION)
                .map(|s| s.to_owned())
                .unwrap_or_default();
            IngressInfo {
                name: name.to_owned(),
                description,
                url,
            }
        })
        .collect();
    ClusterInfo {
        name: cluster_name,
        description: description.clone().unwrap_or_default(),
        ingresses,
    }
}
