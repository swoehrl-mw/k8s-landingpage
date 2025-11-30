use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error: {0}")]
    Generic(String),
    #[error("Kube: {0}")]
    Kube(#[from] kube::Error),
    #[error("MissingKubeconfig: {0}")]
    MissingKubeconfig(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
