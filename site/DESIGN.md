# BDL Pool / BDLMiner instrument theme

The shared direction was approved on 11 November 2026: technical, precise,
accessible to hobbyists. The visual world is an electronics bench and its
measuring instruments. Keep the existing orange chip identity and the actual
product; put detail into the hardware photograph and measured data.

## Audit and treatment

The previous first screens used purple/green radial washes, gradient wordmarks,
glowing actions, translucent pictograms and tilted data panels. Heavy headings,
small tracked capitals and irregular padding reduced the hierarchy. These were
visual problems; the product features and the section order were retained.

1. **Spacing.** Controls, panels and sections now follow a 4 px rhythm. Panels
   use 24 px padding, and related links and readings have consistent gaps.
2. **Color.** Graphite surfaces, three text roles and the logo's Bitcoin orange
   replace the purple/lime chrome. The primary dark-theme action is off-white.
   Green/amber remain semantic states. A complete light palette is included.
3. **Icons.** Existing line icons are unified at 18 px / 1.5 stroke. Shortcut
   pictograms and chat emoji become restrained line icons or text.
4. **Copy.** Headline punctuation is removed. Miner percentages explicitly mean
   share of miners, rather than a positive change. French miner labels are
   harmonised. No performance claims or invented measurements were added.
5. **Type.** Locally served IBM Plex Sans (400/500/600) and IBM Plex Mono
   (400/500) replace heavy, tracked type. Figures use tabular numerals.
6. **Layout.** The first screen places copy beside the product. Pool groups the
   node, service status and timestamp together. Miner presents the existing
   Bitaxe photo with a separate, clearly labelled community reading. Mobile
   follows the reading order. The original sections remain in their order.
7. **Details.** Control and panel radii, tables, selections, focus rings and
   captions share one system. The hashrate chart uses a quiet grid and orange
   series; every actual measurement and missing-data gap is retained.
8. **Visuals.** The photograph is the project's existing
   `installer/assets/boards/gamma.webp`. The on-device previews remain. Pool's
   visual is its actual node verification panel. No image was generated and no
   image prompt was used.
9. **Motion.** Hover feedback uses named color/border transitions. Data panels
   no longer tilt, shortcut links no longer rise, and the progress width no
   longer animates. Existing meaningful loading and accessibility behavior is
   preserved, including reduced motion and keyboard chart inspection.

## Sources and verification

Edit the React sources in `site/` and assets in `installer/assets/`, then run
`npm run build` from `site/` to refresh the tracked `installer/site/` export.
The release packager replaces version/board placeholders as before. No firmware
behavior, API contract or deployment configuration was changed.

The companion BDL Pool implementation is in its own `bdlglua/bdl-pool` source
checkout. It uses the same theme and locally served font assets. Its new shared
`components/Home.tsx` renders both French and English; the existing BTC/BCH,
configuration and monitoring tools keep their original identifiers and behavior.

Validation covers production builds, TypeScript, the existing website browser
suites, responsive layouts, both languages and themes, filters, menus and focus.
Both TypeScript checks and production builds passed. The existing BDLMiner site
suite passed; BDL Pool passed all 26 scenarios (four were rerun serially after
parallel load timeouts). Visual checks at 1440 and 390 px found no horizontal
overflow or homepage JavaScript errors, and both color themes were inspected.
Hardware flashing is outside this visual pass. No production deployment is
performed by this work.

Design method: **unslop by Damir Sokolovsky ([@heydamir](https://x.com/heydamir))**,
licensed CC BY 4.0. https://heydamir.design/unslop
