/** Merge a fetched value after the await so parallel writers see each other's results. */
export async function mergeFetchedRecordEntry<T>(
  read: () => Record<string, T>,
  key: string,
  fetch: Promise<T>,
): Promise<Record<string, T>> {
  const value = await fetch;
  return { ...read(), [key]: value };
}

/** Load every key in parallel, then build the record in one assignment. */
export async function recordFromFetchedKeys<T>(
  keys: string[],
  fetch: (key: string) => Promise<T>,
): Promise<Record<string, T>> {
  const values = await Promise.all(keys.map((key) => fetch(key)));
  return Object.fromEntries(keys.map((key, index) => [key, values[index]]));
}
