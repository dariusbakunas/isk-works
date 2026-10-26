# Third-party notices

ISK Works is licensed under AGPL-3.0-or-later (see `LICENSE`). It builds on
the following, under their own licenses.

## EVEShip.fit dogma engine

`esf-dogma-engine`, `esf-data` (Rust crates, used by `crates/iskworks-fitting`
to calculate fit stats). Copyright (c) EVEShipFit Team. MIT License.
<https://github.com/EVEShipFit/dogma-engine>

## EVEShip.fit patched SDE (`sde.dat`)

The fitting engine data, downloaded at build time from the `@eveshipfit/sde`
npm package (`scripts/fitting-data.sh`) and shipped in the API image. The
EVEShipFit-specific code and data are Copyright (c) EVEShipFit Team, MIT
License. <https://github.com/EVEShipFit/sde-patched>

## EVE Online data

EVE Online static data (in `sde.dat` and the SDE imports) and the EVE
Online name are the property of CCP hf., used under CCP's developer license
agreement: <https://developers.eveonline.com/license-agreement>.
