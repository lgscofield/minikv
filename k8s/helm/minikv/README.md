# minikv Helm chart

## Install

```bash
helm upgrade --install minikv ./k8s/helm/minikv -f k8s/helm/minikv/values-dev.yaml
```

## Profiles

- Dev: values-dev.yaml
- Staging: values-staging.yaml
- Prod: values-prod.yaml

## Examples

```bash
helm upgrade --install minikv ./k8s/helm/minikv -f k8s/helm/minikv/values-staging.yaml
helm upgrade --install minikv ./k8s/helm/minikv -f k8s/helm/minikv/values-prod.yaml
```
