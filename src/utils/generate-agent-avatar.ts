import { createAvatar, type Style } from "@dicebear/core";
import * as collection from "@dicebear/collection";

/** 适合做人设头像的风格（排除 initials / icons 等偏标识类） */
const AVATAR_STYLES = [
  collection.adventurer,
  collection.adventurerNeutral,
  collection.avataaars,
  collection.avataaarsNeutral,
  collection.bigEars,
  collection.bigEarsNeutral,
  collection.bigSmile,
  collection.bottts,
  collection.botttsNeutral,
  collection.croodles,
  collection.croodlesNeutral,
  collection.dylan,
  collection.funEmoji,
  collection.lorelei,
  collection.loreleiNeutral,
  collection.micah,
  collection.miniavs,
  collection.notionists,
  collection.notionistsNeutral,
  collection.openPeeps,
  collection.personas,
  collection.pixelArt,
  collection.pixelArtNeutral,
  collection.thumbs,
  collection.toonHead,
] as Style[];

function pickStyle() {
  return AVATAR_STYLES[Math.floor(Math.random() * AVATAR_STYLES.length)] ?? collection.lorelei;
}

/** 本地 DiceBear 生成智能体头像（data URI）；每次随机风格，seed 影响同风格内细节。 */
export function generateAgentAvatar(seed: string) {
  return createAvatar(pickStyle(), {
    seed: seed.trim() || crypto.randomUUID(),
    size: 128,
  }).toDataUri();
}
