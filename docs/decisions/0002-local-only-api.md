# 2. Refuse non-loopback binding and check Host and Origin

**Status:** Accepted

The specification calls for loopback by default, with CORS limited to dev origins. That protects against the network but not against the user's browser, which sits on the same machine and acts for any page it has open. The API will accept filesystem paths from Phase 1, so a hostile page reaching it matters.

- **DNS rebinding** makes a page same-origin with the API, so CORS does not apply.
- **Cross-site POSTs** still execute under CORS; only reading the response is blocked.

**Decision:**

1. `serve` refuses any non-loopback address. There is no override, since the API has no authentication.
2. Requests must carry a `Host` of `localhost`, `127.0.0.1` or `::1`.
3. Requests with an `Origin` must match the Vite dev server or the host they were sent to. Requests without one, such as curl, are allowed.

**Consequence:** remote access requires a user-managed tunnel such as SSH, and a tunnel must preserve a local `Host`.
