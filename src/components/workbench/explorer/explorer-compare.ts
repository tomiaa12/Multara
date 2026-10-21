import type { ExplorerLexicographic, ExplorerSortOrder } from "./explorer-settings";

let numericCollator: Intl.Collator | null = null;
let accentCollator: Intl.Collator | null = null;

function collatorNumeric() {
  numericCollator ??= new Intl.Collator(undefined, { numeric: true });
  return numericCollator;
}

function collatorNumericCaseInsensitive() {
  accentCollator ??= new Intl.Collator(undefined, { numeric: true, sensitivity: "accent" });
  return accentCollator;
}

const FILE_NAME_MATCH = /^(.*?)(\.([^.]*))?$/;

/** 点文件按名称处理，不当成扩展名。 */
function extractExtension(name: string): string {
  const match = FILE_NAME_MATCH.exec(name);
  return (match && match[1] && match[1].charAt(0) !== "." && match[3]) || "";
}

function compareAndDisambiguateByLength(collator: Intl.Collator, one: string, other: string) {
  const result = collator.compare(one, other);
  if (result !== 0) return result;
  if (one.length !== other.length) return one.length < other.length ? -1 : 1;
  return 0;
}

function startsWithLower(value: string) {
  const character = value.charAt(0);
  return character.toLocaleUpperCase() !== character;
}

function startsWithUpper(value: string) {
  const character = value.charAt(0);
  return character.toLocaleLowerCase() !== character;
}

function compareCaseLowerFirst(one: string, other: string) {
  if (startsWithLower(one) && startsWithUpper(other)) return -1;
  return startsWithUpper(one) && startsWithLower(other) ? 1 : 0;
}

function compareCaseUpperFirst(one: string, other: string) {
  if (startsWithUpper(one) && startsWithLower(other)) return -1;
  return startsWithLower(one) && startsWithUpper(other) ? 1 : 0;
}

export function compareFileNamesDefault(one: string, other: string) {
  return compareAndDisambiguateByLength(collatorNumeric(), one, other);
}

export function compareFileNamesUpper(one: string, other: string) {
  return compareCaseUpperFirst(one, other) || compareAndDisambiguateByLength(collatorNumeric(), one, other);
}

export function compareFileNamesLower(one: string, other: string) {
  return compareCaseLowerFirst(one, other) || compareAndDisambiguateByLength(collatorNumeric(), one, other);
}

export function compareFileNamesUnicode(one: string, other: string) {
  if (one === other) return 0;
  return one < other ? -1 : 1;
}

function compareExtensionsThen(
  one: string,
  other: string,
  middle: (one: string, other: string) => number,
) {
  return (
    compareAndDisambiguateByLength(collatorNumericCaseInsensitive(), extractExtension(one), extractExtension(other)) ||
    middle(one, other) ||
    compareAndDisambiguateByLength(collatorNumeric(), one, other)
  );
}

export function compareFileExtensionsDefault(one: string, other: string) {
  return compareExtensionsThen(one, other, () => 0);
}

export function compareFileExtensionsUpper(one: string, other: string) {
  return compareExtensionsThen(one, other, compareCaseUpperFirst);
}

export function compareFileExtensionsLower(one: string, other: string) {
  return compareExtensionsThen(one, other, compareCaseLowerFirst);
}

export function compareFileExtensionsUnicode(one: string, other: string) {
  const oneExtension = extractExtension(one).toLowerCase();
  const otherExtension = extractExtension(other).toLowerCase();
  if (oneExtension !== otherExtension) return oneExtension < otherExtension ? -1 : 1;
  if (one !== other) return one < other ? -1 : 1;
  return 0;
}

export interface SortableEntry {
  name: string;
  isDirectory: boolean;
  mtime?: number;
  hasNests?: boolean;
}

export interface SortOptions {
  sortOrder: ExplorerSortOrder;
  lexicographic: ExplorerLexicographic;
  reverse: boolean;
}

function comparers(lexicographic: ExplorerLexicographic) {
  switch (lexicographic) {
    case "upper":
      return { names: compareFileNamesUpper, extensions: compareFileExtensionsUpper };
    case "lower":
      return { names: compareFileNamesLower, extensions: compareFileExtensionsLower };
    case "unicode":
      return { names: compareFileNamesUnicode, extensions: compareFileExtensionsUnicode };
    default:
      return { names: compareFileNamesDefault, extensions: compareFileExtensionsDefault };
  }
}

/** VS Code `FileSorter.compare` 的移植（不含工作区根的排序，根按添加顺序）。 */
export function compareExplorerEntries(a: SortableEntry, b: SortableEntry, options: SortOptions): number {
  if (options.reverse) [a, b] = [b, a];
  const { names, extensions } = comparers(options.lexicographic);

  switch (options.sortOrder) {
    case "type":
      if (a.isDirectory && !b.isDirectory) return -1;
      if (b.isDirectory && !a.isDirectory) return 1;
      if (a.isDirectory && b.isDirectory) return names(a.name, b.name);
      break;
    case "filesFirst":
      if (a.isDirectory && !b.isDirectory) return 1;
      if (b.isDirectory && !a.isDirectory) return -1;
      break;
    case "foldersNestsFiles":
      if (a.isDirectory && !b.isDirectory) return -1;
      if (b.isDirectory && !a.isDirectory) return 1;
      if (a.hasNests && !b.hasNests) return -1;
      if (b.hasNests && !a.hasNests) return 1;
      break;
    case "mixed":
      break;
    default:
      if (a.isDirectory && !b.isDirectory) return -1;
      if (b.isDirectory && !a.isDirectory) return 1;
      break;
  }

  switch (options.sortOrder) {
    case "type":
      return extensions(a.name, b.name);
    case "modified":
      if (a.mtime !== b.mtime) return a.mtime && b.mtime && a.mtime < b.mtime ? 1 : -1;
      return names(a.name, b.name);
    default:
      return names(a.name, b.name);
  }
}

export function sortExplorerEntries<T extends SortableEntry>(entries: readonly T[], options: SortOptions): T[] {
  return [...entries].sort((a, b) => compareExplorerEntries(a, b, options));
}
