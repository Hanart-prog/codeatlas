# 3. Pin frontend tools to majors that support Node 20.16, run pnpm through npx

**Status:** Accepted

The development machine has Node 20.16. Current majors of Vite, plugin-react, ESLint, jsdom and jest-dom require Node 20.19 or 22. Corepack 0.28.2 also fails signature verification when resolving pnpm.

**Decision:** stay on Vite 6, plugin-react 4, Vitest 3, ESLint 9, jsdom 26, jest-dom 6 and TypeScript 5.9, which all support Node 20.16. `make` uses `pnpm` when installed, otherwise the pinned version through `npx`.

**Consequence:** no system changes are needed. Moving to Node 22 later permits a coordinated upgrade of these tools.
