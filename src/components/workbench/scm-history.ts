export type HistoryCommit = {
  hash: string;
  parents: string[];
  subject: string;
  author: string;
  committedAt: string;
  refs: string[];
};

export type HistoryRefKind = "head" | "branch" | "remote" | "tag";

export type HistoryRef = {
  name: string;
  kind: HistoryRefKind;
};

export type HistoryLink = {
  from: number;
  to: number;
  color: number;
};

export type HistoryTrack = {
  column: number;
  color: number;
};

export type HistoryRowLayout = {
  commit: HistoryCommit;
  column: number;
  color: number;
  incoming: boolean;
  tracks: HistoryTrack[];
  links: HistoryLink[];
};

export const SCM_LANE_WIDTH = 11;
/** 轮转泳道色数量（scmGraph.foreground1–5） */
export const SCM_LANE_COLORS = 5;
/** 对齐 VS Code：当前分支 / 远端跟踪 / merge-base */
export const SCM_COLOR_REF = 5;
export const SCM_COLOR_REMOTE = 6;
export const SCM_COLOR_BASE = 7;

export type HistoryColorMap = Map<string, number | undefined>;

/** 当前分支→蓝、上游→紫；其余 ref 占位，徽章跟泳道色。 */
export function historyColorMap(branch?: string, upstream?: string | null): HistoryColorMap {
  const map: HistoryColorMap = new Map();
  const head = branch?.trim();
  if (head) map.set(head, SCM_COLOR_REF);
  const remote = upstream?.trim();
  if (remote) map.set(remote, SCM_COLOR_REMOTE);
  return map;
}

function labelColorOf(commit: HistoryCommit, colorMap: HistoryColorMap): number | undefined {
  for (const ref of historyRefs(commit)) {
    if (!colorMap.has(ref.name)) continue;
    const color = colorMap.get(ref.name);
    if (color !== undefined) return color;
  }
  if (isHistoryHead(commit) && ![...colorMap.values()].includes(SCM_COLOR_REF)) {
    return SCM_COLOR_REF;
  }
  return undefined;
}

function badgeColorOf(ref: HistoryRef, circleColor: number, colorMap: HistoryColorMap): number {
  if (colorMap.has(ref.name)) {
    const mapped = colorMap.get(ref.name);
    if (mapped !== undefined) return mapped;
    return circleColor;
  }
  if (ref.kind === "head") return SCM_COLOR_REF;
  if (ref.kind === "tag") return SCM_COLOR_BASE;
  return circleColor;
}

export function parseHistoryRef(raw: string): HistoryRef {
  if (raw.startsWith("HEAD -> ")) return { name: raw.slice("HEAD -> ".length), kind: "head" };
  if (raw === "HEAD") return { name: "HEAD", kind: "head" };
  if (raw.startsWith("tag: ")) return { name: raw.slice("tag: ".length), kind: "tag" };
  if (raw.includes("/")) return { name: raw, kind: "remote" };
  return { name: raw, kind: "branch" };
}

export function historyRefs(commit: HistoryCommit): HistoryRef[] {
  return commit.refs.map(parseHistoryRef).filter((item) => item.name);
}

export const HISTORY_REF_ICON: Record<HistoryRefKind, string> = {
  head: "target",
  branch: "git-branch",
  remote: "cloud",
  tag: "tag",
};

export type HistoryRefBadge = {
  key: string;
  kind: HistoryRefKind;
  name: string;
  count: number;
  title: string;
  color: number;
};

/** First ref shows its name. The rest collapse by kind into an icon, with a count when several share it. */
export function historyRefBadges(
  commit: HistoryCommit,
  circleColor = SCM_COLOR_REF,
  colorMap: HistoryColorMap = new Map(),
): HistoryRefBadge[] {
  const refs = historyRefs(commit);
  const first = refs[0];
  if (!first) return [];
  const badges: HistoryRefBadge[] = [{
    key: `${first.kind}:${first.name}`,
    kind: first.kind,
    name: first.name,
    count: 0,
    title: first.name,
    color: badgeColorOf(first, circleColor, colorMap),
  }];
  const grouped = new Map<HistoryRefKind, HistoryRef[]>();
  for (const item of refs.slice(1)) {
    const list = grouped.get(item.kind);
    if (list) list.push(item);
    else grouped.set(item.kind, [item]);
  }
  for (const [kind, items] of grouped) {
    badges.push({
      key: kind,
      kind,
      name: "",
      count: items.length > 1 ? items.length : 0,
      title: items.map((item) => item.name).join("\n"),
      color: badgeColorOf(items[0], circleColor, colorMap),
    });
  }
  return badges;
}

export function isHistoryHead(commit: HistoryCommit) {
  return historyRefs(commit).some((item) => item.kind === "head");
}

export function layoutHistory(commits: HistoryCommit[]): HistoryRowLayout[] {
  let columns: Array<string | null> = [];
  const colorOf = new Map<string, number>();
  let nextColor = 0;
  const colorFor = (hash: string) => {
    const existing = colorOf.get(hash);
    if (existing != null) return existing;
    const color = nextColor % SCM_LANE_COLORS;
    nextColor += 1;
    colorOf.set(hash, color);
    return color;
  };
  const rows: HistoryRowLayout[] = [];

  for (const commit of commits) {
    const incoming = columns.includes(commit.hash);
    let column = columns.indexOf(commit.hash);
    if (column === -1) {
      column = columns.findIndex((item) => item == null);
      if (column === -1) {
        column = columns.length;
        columns.push(commit.hash);
      } else {
        columns[column] = commit.hash;
      }
    }
    const color = colorFor(commit.hash);
    const next = columns.slice();
    const parents = commit.parents.filter(Boolean);
    const links: HistoryLink[] = [];

    if (!parents.length) {
      next[column] = null;
    } else {
      const first = parents[0];
      colorFor(first);
      const existing = next.indexOf(first);
      if (existing !== -1 && existing !== column) {
        next[column] = null;
        links.push({ from: column, to: existing, color: colorOf.get(first) ?? color });
      } else {
        next[column] = first;
        links.push({ from: column, to: column, color: colorOf.get(first) ?? color });
      }
      for (const parent of parents.slice(1)) {
        const parentColor = colorFor(parent);
        let target = next.indexOf(parent);
        if (target === -1) {
          target = next.findIndex((item) => item == null);
          if (target === -1) {
            target = next.length;
            next.push(parent);
          } else {
            next[target] = parent;
          }
        }
        links.push({ from: column, to: target, color: parentColor });
      }
    }

    const tracks = columns.flatMap((hash, index) => {
      if (!hash || index === column) return [];
      return [{ column: index, color: colorOf.get(hash) ?? 0 }];
    });
    rows.push({ commit, column, color, incoming, tracks, links });
    columns = next;
    while (columns.length && columns[columns.length - 1] == null) columns.pop();
  }

  return rows;
}

export function historyLaneCount(rows: HistoryRowLayout[]) {
  let max = 0;
  for (const row of rows) {
    max = Math.max(max, row.column, ...row.tracks.map((track) => track.column), ...row.links.map((link) => link.to));
  }
  return rows.length ? max + 1 : 0;
}

export const SCM_LANE_HEIGHT = 22;

export type HistoryLane = {
  id: string;
  color: number;
};

export type HistoryGraphRow = {
  commit: HistoryCommit;
  kind: "head" | "node";
  input: HistoryLane[];
  output: HistoryLane[];
};

export type HistoryGraphPath = {
  d: string;
  color: number;
  width: number;
};

export type HistoryGraphDot = {
  index: number;
  r: number;
  color: number;
  hole: boolean;
};

const CURVE_RADIUS = 5;
const CIRCLE_RADIUS = 4;

function laneX(index: number) {
  return SCM_LANE_WIDTH * (index + 1);
}

function findLastLane(lanes: HistoryLane[], id: string) {
  for (let index = lanes.length - 1; index >= 0; index -= 1) {
    if (lanes[index].id === id) return index;
  }
  return -1;
}

export function layoutHistoryGraph(
  commits: HistoryCommit[],
  colorMap: HistoryColorMap = new Map(),
): HistoryGraphRow[] {
  let colorIndex = -1;
  const nextLaneColor = () => {
    colorIndex = (colorIndex + 1) % SCM_LANE_COLORS;
    return colorIndex;
  };
  const byHash = new Map(commits.map((item) => [item.hash, item]));
  const map: HistoryColorMap = new Map(colorMap);
  // 未映射的 ref 占位为 undefined，徽章回落到该 commit 泳道色（对齐 VS Code）
  for (const commit of commits) {
    for (const ref of historyRefs(commit)) {
      if (!map.has(ref.name)) map.set(ref.name, undefined);
    }
  }
  const rows: HistoryGraphRow[] = [];
  for (const commit of commits) {
    const input = (rows.at(-1)?.output ?? []).map((lane) => ({ ...lane }));
    const output: HistoryLane[] = [];
    const parents = commit.parents.filter(Boolean);
    let firstParentAdded = false;
    if (parents.length) {
      for (const lane of input) {
        if (lane.id === commit.hash) {
          if (!firstParentAdded) {
            output.push({
              id: parents[0],
              color: labelColorOf(commit, map) ?? lane.color,
            });
            firstParentAdded = true;
          }
          continue;
        }
        output.push({ ...lane });
      }
    }
    for (let index = firstParentAdded ? 1 : 0; index < parents.length; index += 1) {
      let labeled: number | undefined;
      if (index === 0) labeled = labelColorOf(commit, map);
      else {
        const parent = byHash.get(parents[index]);
        if (parent) labeled = labelColorOf(parent, map);
      }
      output.push({ id: parents[index], color: labeled ?? nextLaneColor() });
    }
    rows.push({
      commit,
      kind: isHistoryHead(commit) ? "head" : "node",
      input,
      output,
    });
  }
  return rows;
}

export function historyCircleColor(row: HistoryGraphRow) {
  const index = historyCircleIndex(row);
  if (index < row.output.length) return row.output[index].color;
  if (index < row.input.length) return row.input[index].color;
  return SCM_COLOR_REF;
}

export function historyCircleIndex(row: HistoryGraphRow) {
  const inputIndex = row.input.findIndex((lane) => lane.id === row.commit.hash);
  return inputIndex === -1 ? row.input.length : inputIndex;
}

/** 单行图宽：跟 VS Code 一样按本行泳道数算，文字从圆点右侧起排，不跟全表最大宽度对齐。 */
export function historyRowGraphWidth(row: HistoryGraphRow) {
  return SCM_LANE_WIDTH * (Math.max(row.input.length, row.output.length, 1) + 1);
}

export function historyGraphWidth(rows: HistoryGraphRow[]) {
  let width = SCM_LANE_WIDTH * 2;
  for (const row of rows) {
    width = Math.max(width, historyRowGraphWidth(row));
  }
  return width;
}

export function historyGraph(row: HistoryGraphRow, expanded = false): { paths: HistoryGraphPath[]; dots: HistoryGraphDot[] } {
  const parents = row.commit.parents.filter(Boolean);
  const inputIndex = row.input.findIndex((lane) => lane.id === row.commit.hash);
  const circleIndex = inputIndex === -1 ? row.input.length : inputIndex;
  const circleColor = circleIndex < row.output.length
    ? row.output[circleIndex].color
    : circleIndex < row.input.length
      ? row.input[circleIndex].color
      : 0;
  const paths: HistoryGraphPath[] = [];
  let outputIndex = 0;

  for (let index = 0; index < row.input.length; index += 1) {
    const color = row.input[index].color;
    if (row.input[index].id === row.commit.hash) {
      if (index !== circleIndex) {
        const x = laneX(index);
        paths.push({
          d: `M ${x} 0 A ${SCM_LANE_WIDTH} ${SCM_LANE_WIDTH} 0 0 1 ${x - SCM_LANE_WIDTH} ${SCM_LANE_WIDTH} H ${laneX(circleIndex)}`,
          color,
          width: 1,
        });
      } else {
        outputIndex += 1;
      }
      continue;
    }
    if (outputIndex < row.output.length && row.input[index].id === row.output[outputIndex].id) {
      if (index === outputIndex) {
        paths.push({ d: `M ${laneX(index)} 0 V ${SCM_LANE_HEIGHT}`, color, width: 1 });
      } else {
        const from = laneX(index);
        const to = laneX(outputIndex);
        paths.push({
          d: `M ${from} 0 V 6 A ${CURVE_RADIUS} ${CURVE_RADIUS} 0 0 1 ${from - CURVE_RADIUS} ${SCM_LANE_HEIGHT / 2} H ${to + CURVE_RADIUS} A ${CURVE_RADIUS} ${CURVE_RADIUS} 0 0 0 ${to} ${SCM_LANE_HEIGHT / 2 + CURVE_RADIUS} V ${SCM_LANE_HEIGHT}`,
          color,
          width: 1,
        });
      }
      outputIndex += 1;
    }
  }

  for (let index = 1; index < parents.length; index += 1) {
    const parentIndex = findLastLane(row.output, parents[index]);
    if (parentIndex === -1) continue;
    const x = SCM_LANE_WIDTH * parentIndex;
    paths.push({
      d: `M ${x} ${SCM_LANE_HEIGHT / 2} A ${SCM_LANE_WIDTH} ${SCM_LANE_WIDTH} 0 0 1 ${x + SCM_LANE_WIDTH} ${SCM_LANE_HEIGHT} M ${x} ${SCM_LANE_HEIGHT / 2} H ${laneX(circleIndex)}`,
      color: row.output[parentIndex].color,
      width: 1,
    });
  }

  if (inputIndex !== -1) {
    paths.push({
      d: `M ${laneX(circleIndex)} 0 V ${SCM_LANE_HEIGHT / 2}`,
      color: row.input[inputIndex].color,
      width: 1,
    });
  }
  if (parents.length) {
    paths.push({
      d: `M ${laneX(circleIndex)} ${SCM_LANE_HEIGHT / 2} V ${SCM_LANE_HEIGHT}`,
      color: circleColor,
      width: expanded ? 3 : 1,
    });
  }

  // 圆点半径对齐 VS Code：HEAD 外圈 7 / 内圈 2；多父提交外 6 / 内 3；普通 5。
  const dots: HistoryGraphDot[] = row.kind === "head"
    ? [
        { index: circleIndex, r: CIRCLE_RADIUS + 3, color: circleColor, hole: false },
        { index: circleIndex, r: 2, color: circleColor, hole: true },
      ]
    : parents.length > 1
      ? [
          { index: circleIndex, r: CIRCLE_RADIUS + 2, color: circleColor, hole: false },
          { index: circleIndex, r: CIRCLE_RADIUS - 1, color: circleColor, hole: false },
        ]
      : [{ index: circleIndex, r: CIRCLE_RADIUS + 1, color: circleColor, hole: false }];

  return { paths, dots };
}

export function historyGraphRails(lanes: HistoryLane[], highlight?: number): HistoryGraphPath[] {
  return lanes.map((lane, index) => ({
    d: `M ${laneX(index)} 0 V ${SCM_LANE_HEIGHT}`,
    color: lane.color,
    width: index === highlight ? 3 : 1,
  }));
}
