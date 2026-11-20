//! Docker commands for console-owned containers.
use tokio::process::Command;

pub(crate) fn host() -> String {
    std::env::var("PHYXI_DOCKER_HOST").unwrap_or_else(|_| "unix:///var/run/docker.sock".into())
}

pub(crate) fn command(host: &str, args: &[String]) -> Command {
    let mut c = Command::new("docker");
    for key in [
        "DOCKER_HOST",
        "DOCKER_CONTEXT",
        "DOCKER_TLS_VERIFY",
        "DOCKER_TLS",
        "DOCKER_API_VERSION",
        "DOCKER_CERT_PATH",
        "DOCKER_CONFIG",
    ] {
        c.env_remove(key);
    }
    c.arg("--host").arg(host).args(args);
    c
}
