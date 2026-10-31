export function formatDate(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toISOString().slice(0, 10);
}

export function shortOid(oid: string): string {
  return oid.slice(0, 10);
}

export function formatCount(value: number): string {
  return value.toLocaleString("en-US");
}
