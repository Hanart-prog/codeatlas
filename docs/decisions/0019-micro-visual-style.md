# 19. Adopt the Micro visual style, light first

**Status:** Accepted. Replaces "dark-first" in section 13 of the specification, at the owner's request.

**Decision:** the dashboard takes its colors, radii, shadows and component shapes from the [Micro style reference](https://styles.refero.design/style/cc43cfe3-195b-4081-b586-c42db054a466). Light is the default, and the saved dark choice still applies.

That reference covers a light marketing site, so these parts are our own:

- A dark theme built from the same warm neutrals.
- Inter and JetBrains Mono, its documented fallbacks, because its own fonts are commercial. They are bundled, so the dashboard loads nothing from the network.
- Secondary text darker than its grays, and darkened status colors, because some of its values miss 4.5:1. `contrast.test.ts` checks every text and surface pair in both themes.
- Status colors kept apart from the decorative marker colors.
- The gradient appears only as a thin line at the top of the app. Chart colors keep their validated blue, because the reference has no chart rules.

**Consequence:** Vite must not inline small assets, because the content security policy blocks `data:` fonts. The end-to-end security test now fails on any console error.
