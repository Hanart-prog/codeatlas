import { useSyncExternalStore } from "react";

import { type AppRoute, ROUTES } from "./routes";

function subscribe(onChange: () => void) {
  window.addEventListener("hashchange", onChange);
  return () => window.removeEventListener("hashchange", onChange);
}

function currentRoute(): AppRoute {
  const id = window.location.hash.replace(/^#\/?/, "");
  return ROUTES.find((route) => route.id === id) ?? ROUTES[0];
}

export function useHashRoute(): AppRoute {
  return useSyncExternalStore(subscribe, currentRoute);
}
