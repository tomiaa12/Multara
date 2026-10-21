import { addCollection } from "@iconify/vue/offline";
import { icons } from "@iconify-json/vscode-icons";
import { getIconForFile, getIconForFolder, getIconForOpenFolder } from "vscode-icons-js";
import type { FileIconResolver } from "./types";

addCollection(icons);

const iconNames = new Set([
  ...Object.keys(icons.icons),
  ...Object.keys(icons.aliases ?? {}),
]);

function toIconifyId(fileName: string | undefined): string | null {
  if (!fileName) return null;
  const name = fileName.replace(/\.svg$/i, "").replaceAll("_", "-");
  return iconNames.has(name) ? `vscode-icons:${name}` : null;
}

export const resolver: FileIconResolver = ({ name, directory, open }) => {
  if (directory) {
    return {
      iconify: toIconifyId(open ? getIconForOpenFolder(name) : getIconForFolder(name))
        ?? (open ? "vscode-icons:default-folder-opened" : "vscode-icons:default-folder"),
    };
  }
  return { iconify: toIconifyId(getIconForFile(name)) ?? "vscode-icons:default-file" };
};
