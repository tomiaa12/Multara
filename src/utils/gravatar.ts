import { md5Hex } from "./md5";

/** Gravatar：邮箱 MD5；无邮箱时返回空串，由 UI 回退首字母。 */
export function gravatarUrl(email: string, size = 40): string {
  const normalized = email.trim().toLowerCase();
  if (!normalized) return "";
  return `https://www.gravatar.com/avatar/${md5Hex(normalized)}?s=${size}&d=identicon`;
}

export function authorInitial(name: string): string {
  const trimmed = name.trim();
  return trimmed ? trimmed.slice(0, 1).toUpperCase() : "?";
}
