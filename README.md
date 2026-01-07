# K8s Landingpage

K8s-Landingpage is a lightweight application that scans your Kubernetes clusters, local or remote via Cluster API, and renders a simple, customizable landing page listing every published Ingress alongside friendly metadata.

Ship it with the provided Helm chart, plug in optional OIDC auth, and tailor the UI by dropping in your own template or static assets. It’s ideal for platform teams that want a central, discoverable catalog of ingress endpoints without wiring together dashboards or maintaining bespoke tooling.

## Getting started

You can install k8s-landingpage into any current Kubernetes cluster using helm:

```bash
helm install k8s-landingpage oci://ghcr.io/swoehrl-mw/helm/k8s-landingpage --wait
```

Then start a port-forward to access the landingpage:

```bash
kubectl port-forward svc/k8s-landingpage 8000:8000
```

Now you can open [http://localhost:8000](http://localhost:8000) in your browser and should see a list of all `Ingresses` defined in your cluster.

## Configuration

This tool can list ingresses both for the local cluster and for any connected remote clusters managed by [Cluster API](https://cluster-api.sigs.k8s.io/). It does this by reading the kubeconfig from a secret, connecting to that cluster and listing `Ingress` objects.

The following configuration options for the Helm Chart are available:

```yaml
config:
  global:
    refreshIntervalSeconds: 30  # How often should the controller refresh the list of ingress objects
    onlyWithAnnotation: false  # Only list ingress objects with specific annotations (see below)

  local:
    enabled: true  # Collect ingress objects from the local cluster (requires RBAC permissions)

  # A list of remote clusters to collect ingress objects from
  remote:
    groupname: # A group of clusters (e.g. all prod clusters)
      - name: foobar  # The name of the cluster
        description:  # An optional description to show beside the cluster name
        kubeconfigSecret:
          name: foobar  # The name of the secret that contains a key "value" with the kubeconfig to access the remote cluster
          namespace: default  # Namespace the secret is placed in
```

You can use annotations on the ingress objects to provide easy to understand names and descriptions. The annotations are `landingpage.info/name` and `landingpage.info/description`.
By default the tool will list all ingress objects it finds. If you set `config.global.onlyWithAnnotation` to `true`, it will filter out any that do not have either of the landingpage annotations.

The helm chart creates a custom `ClusterRole` with permissions to read `Ingress` and `Secret` objects in the entire cluster. You might want to create your own more restricted role and serviceaccount and point the tool to them via the following Helm Chart values:

```yaml
serviceAccount:
  # Disable creating a serviceaccount
  create: false
  # Give the name of your precreated serviceaccount
  name: my-custom-serviceaccount
  # Disable creation of default ClusterRole
  rbac: false
```

### OIDC

K8s-Landingpage has experimental support for [OIDC](https://openid.net/developers/how-connect-works/) authentication to protect the landingpage. To use it, create a client in your Identity Provider (tested with [Dex](https://dexidp.io/)) with a client ID and secret. Store both in a secret (with keys `clientId` and `clientSecret`), then add the following Helm Chart values:

```yaml
oidc:
  enabled: true
  issuer: https://auth.mycompany.com  # URL of the OIDC Identitiy Provider Issuer URL
  secret: landingpage-oidc  # Name of the secret that contains keys "clientId" and "clientSecret"
  baseUrl: https://landingpage  # Base URL this app is served under (use it also for the Identitiy Provider Redirect URL)
  renewalInterval: # Optional, interval in seconds after which to reload OIDC discovery URL. Use if your Identity Provider rotates keys regularly (like Dex does)
```

Currently you must still create your own ingress to expose the landingpage.

After configuration is complete, when first opening the landingpage you will automatically get redirected to your Identiy Provider for login.

Note that any static assets are not protected by the login, so make sure they don't contain sensitive information.

### Customizing the page

You can and should customize the design of the landingpage. To do so, write your own main template HTML (for the default template and to see what variables are available see the `template.html` in this repository). For templating this tool uses [minijinja](https://docs.rs/minijinja/latest/minijinja/index.html), see its docs for available functions and mechanisms. Note that currently the tool does not support using multiple templates.
You can also add supporting static assets like CSS or images. These will be served under the path `static/`.

Once you are finished, create a `ConfigMap` with a key `template.html` for the main template and a second `ConfigMap` for all the static assets (the key becomes the filename). Then add the following Helm Chart values:

```yaml
templateConfigMap: my-template  # Name of the ConfigMap with the main template
staticConfigMap: my-assets  # Name of the ConfigMap with supporting static assets
```

## Local development

This tools is developed in [Rust](https://rust-lang.org/learn/get-started/). You need a current Rust+Cargo toolchain for local development.

For testing you also need access to a Kubernetes cluster. If needed you can setup a local one using [K3d](https://k3d.io) and add some dummy Ingress objects to it. Then adapt the `config.yaml` in this repository to your liking and run `cargo run`. The landingpage will be available under [http://localhost:8000](http://localhost:8000).
