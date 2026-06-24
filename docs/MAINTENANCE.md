# Maintenance

## Update dependencies

### Go (trainbot)

- in [../Makefile](../Makefile), bump `DOCKER_BASE_IMAGE` and `GO_VERSION`
- `go get -u ./...`
- `go mod tidy`
- `make docker_lint docker_test`
- `make deploy_trainbot host=…`

### JS Frontend

- `cd frontend`
- in [Makefile](../frontend/Makefile), bump `DOCKER_BASE_IMAGE`
- `npm update`
- `npm outdated`
- for major bumps: `npx npm-check-updates -u && npm install`
- `source env && make docker_build`
- `source env && make deploy`

### Nix development env

- in [../flake.nix](../flake.nix), bump `nixpkgs.url` and go and node build input packages
- `nix flake update`
- `nix develop --command make lint test`
