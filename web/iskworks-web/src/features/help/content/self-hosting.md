# Self-hosting

ISK Works is built to run as your own instance, for yourself or your corporation. The public Docker images are:

- `dariusbakunas/iskworks-api`: the API
- `dariusbakunas/iskworks-worker`: background ESI sync
- `dariusbakunas/iskworks-web`: this web app
- `dariusbakunas/iskworks-sde-import`: imports EVE's Static Data Export

The full self-hosting guide (Docker Compose stack, EVE SSO setup, SDE imports, backups) is `deploy/README.md` in the [source code](source:). ISK Works is licensed under the GNU AGPL v3: if you run a modified version for others, you must offer them its source.

> Every instance needs its **own EVE developer application** (SSO client ID and secret) from [developers.eveonline.com](https://developers.eveonline.com). Creating one means you accept CCP's Developer License Agreement as that instance's operator.
