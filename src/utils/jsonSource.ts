export function formatJsonSource(source: unknown, expandJsonObjectStrings = false): string {
  const formatted = JSON.stringify(
    expandJsonObjectStrings ? expandNestedJsonObjectStrings(source) : source,
    null,
    2,
  );
  return formatted ?? '';
}

export function expandNestedJsonObjectStrings(value: unknown): unknown {
  if (typeof value === 'string') {
    const parsed = parseCompleteJsonObject(value);
    return parsed === undefined ? value : expandNestedJsonObjectStrings(parsed);
  }
  if (Array.isArray(value)) return value.map(expandNestedJsonObjectStrings);
  if (!isJsonObject(value)) return value;
  return Object.fromEntries(
    Object.entries(value).map(([key, item]) => [key, expandNestedJsonObjectStrings(item)]),
  );
}

export function containsNestedJsonObjectString(value: unknown): boolean {
  if (typeof value === 'string') return parseCompleteJsonObject(value) !== undefined;
  if (Array.isArray(value)) return value.some(containsNestedJsonObjectString);
  return isJsonObject(value) && Object.values(value).some(containsNestedJsonObjectString);
}

function parseCompleteJsonObject(value: string): Record<string, unknown> | undefined {
  const trimmed = value.trim();
  if (!trimmed.startsWith('{') || !trimmed.endsWith('}')) return undefined;
  try {
    const parsed: unknown = JSON.parse(trimmed);
    return isJsonObject(parsed) ? parsed : undefined;
  } catch {
    return undefined;
  }
}

function isJsonObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
