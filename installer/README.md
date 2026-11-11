# Public website

The native Rust packager builds the stable installer, opt-in beta installer and
signed update pointers. The browser application is Rust compiled to WebAssembly.
See [build and test instructions](../site/README.md).

Shared HTML templates live in `site/templates/`; Rust browser modules and their
French/English dictionaries live in `site/browser/`. Generated pages work at the
custom domain, under `/BDLMinerV2/` and under `/beta/`. Navigation and preferences
are shared. Board links open `flash.html?board=<profile>` with a validated profile.

## Discovery content

`content.json` supplies the board comparison, donation destination, release
highlights and reviewed community gallery. The site's English/French copy is
in `../site/browser/discovery.json`. Keep measured hashrates tied to the evidence linked in
each board entry; leave unmeasured targets without a `rate` value.

`pool_names` and `pool_links` describe the recognised public pool groups and their
official website links. Keep them aligned with the Rust pool catalog in `BDLMiner-stats`.
Links must use HTTPS on the group's public domain, without credentials, query or
fragment. Never add links for `other` or `unknown`, or use an endpoint from a report.

The support link is the owner's supplied GitHub Sponsors address. No address
from a pool configuration or statistics sample is reused as a donation address.
Bitcoin/Lightning fields are optional and remain empty unless supplied by the
owner. A site with no configured donation destination hides the support section.

Identification images and their source licences are in `assets/boards/`.
They are distinct from community submissions and physical BDLMiner measurements.

## Community photos

The gallery starts empty. The site's submission button opens
`.github/ISSUE_TEMPLATE/community_setup.yml`; publication requires manual review,
the author's permission, an attribution, the board model and the original
Reddit/Discord link. No public upload endpoint or automatic publication is added.

After review, save an optimized photo under `installer/assets/community/` and add
an entry to `content.json`:

```json
{
  "board": "M5StickC Plus2",
  "image": "assets/community/approved-setup.webp",
  "credit": "Contributor's chosen name",
  "caption": {"en": "Contributor-approved description", "fr": "Description approuvée"},
  "url": "https://www.reddit.com/r/bdlminer/comments/POST_ID/",
  "permission": true
}
```

Keep the permission record in the submission. Do not invent installations or
reuse other people's photos without permission.

## Interactive demos

The Rust packager reuses the miner dashboard, settings/log HTML and fleet hub
HTML/CSS in `demo/miner/` and `demo/hub/`. `site/browser/src/demo.rs` supplies
fictional in-memory state. A Content Security Policy restricts connections to
the same origin, permitting WASM loading while blocking external and LAN APIs.
Hardware-only actions explain that a real miner is required.

The demo banner, reserved example addresses and fictional boot ID distinguish
these readings from measurements. The firmware UI itself is unchanged.

Run `node test/web/test_site.cjs` for both packaged channels and hosting prefixes.
Native packaging contracts are tested with `cargo test --locked --manifest-path
site/Cargo.toml`.
