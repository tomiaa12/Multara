/** 浏览器里没有 Rust 后端时，工作流保存在这次页面的内存里。 */
import { t } from "../i18n";
import { emptyWorkflowDef, type NodeTemplate, type SecretName, type VersionMeta, type Workflow, type WorkflowDef, type WorkflowDetail, type WorkflowSummary } from "../domain/workflow";
import { fixtureDefinition, fixtureDetail, fixtureNodeTemplates, fixtureSecrets, fixtureWorkflows } from "./workflow-fixtures";

interface Entry {
  workflow: Workflow;
  draft: WorkflowDef;
  published: WorkflowDef | null;
  versions: VersionMeta[];
}

let entries: Entry[] | null = null;
let templates: NodeTemplate[] | null = null;
let secrets: SecretName[] | null = null;

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function now() {
  return new Date().toISOString();
}

function slugify(name: string) {
  const slug = name.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  return slug || "flow";
}

function store(): Entry[] {
  if (entries) return entries;
  entries = fixtureWorkflows().map((workflow) => {
    const detail = fixtureDetail(workflow.id);
    return {
      workflow: clone(detail.workflow),
      draft: clone(detail.draft),
      published: detail.workflow.publishedVersion ? clone(detail.draft) : null,
      versions: clone(detail.versions),
    };
  });
  return entries;
}

function templateStore(): NodeTemplate[] {
  if (!templates) templates = clone(fixtureNodeTemplates());
  return templates;
}

function requireEntry(id: string): Entry {
  const entry = store().find((item) => item.workflow.id === id);
  if (!entry) throw new Error(t("workflow.editor.notFound"));
  return entry;
}

function summary(entry: Entry): WorkflowSummary {
  return {
    ...entry.workflow,
    lastRunStatus: null,
    lastRunAt: null,
    nodeCount: entry.draft.nodes.length,
  };
}

export function memoryListWorkflows(): WorkflowSummary[] {
  return store().map(summary);
}

export function memoryGetWorkflow(id: string): WorkflowDetail {
  const entry = requireEntry(id);
  return {
    workflow: clone(entry.workflow),
    draft: clone(entry.draft),
    published: entry.published ? clone(entry.published) : null,
    versions: clone(entry.versions),
    diagnostics: [],
  };
}

export function memoryCreateWorkflow(name: string, description = "", templateId?: string | null): Workflow {
  const draft = templateId === "flow.requirement_to_release" ? fixtureDefinition() : emptyWorkflowDef();
  const stamp = now();
  const workflow: Workflow = {
    id: `wf-${crypto.randomUUID()}`,
    name: name.trim() || t("workflow.list.blank"),
    slug: slugify(name),
    description,
    icon: "",
    publishedVersion: null,
    enabled: false,
    draftUpdatedAt: stamp,
    createdAt: stamp,
    updatedAt: stamp,
    hasUnpublishedChanges: true,
  };
  store().unshift({ workflow, draft: clone(draft), published: null, versions: [] });
  return clone(workflow);
}

export function memoryUpdateWorkflowMeta(id: string, name: string, description: string, icon = ""): Workflow {
  const entry = requireEntry(id);
  entry.workflow.name = name;
  entry.workflow.slug = slugify(name);
  entry.workflow.description = description;
  entry.workflow.icon = icon;
  entry.workflow.updatedAt = now();
  return clone(entry.workflow);
}

export function memorySetWorkflowEnabled(id: string, enabled: boolean) {
  const entry = requireEntry(id);
  if (enabled && !entry.workflow.publishedVersion) throw new Error(t("workflow.list.enableNeedsPublish"));
  entry.workflow.enabled = enabled;
  entry.workflow.updatedAt = now();
}

export function memorySaveWorkflowDraft(id: string, draft: WorkflowDef): string {
  const entry = requireEntry(id);
  entry.draft = clone(draft);
  const stamp = now();
  entry.workflow.draftUpdatedAt = stamp;
  entry.workflow.updatedAt = stamp;
  entry.workflow.hasUnpublishedChanges = true;
  return stamp;
}

export function memoryPublishWorkflow(id: string, note: string): number {
  const entry = requireEntry(id);
  const version = (entry.workflow.publishedVersion ?? 0) + 1;
  entry.workflow.publishedVersion = version;
  entry.workflow.hasUnpublishedChanges = false;
  entry.workflow.updatedAt = now();
  entry.published = clone(entry.draft);
  entry.versions.unshift({ version, note, createdAt: entry.workflow.updatedAt });
  return version;
}

export function memoryDeleteWorkflow(id: string) {
  const list = store();
  const index = list.findIndex((item) => item.workflow.id === id);
  if (index >= 0) list.splice(index, 1);
}

export function memoryDuplicateWorkflow(id: string): Workflow {
  const entry = requireEntry(id);
  return memoryCreateFrom(entry.workflow.name, entry.workflow.description, entry.draft);
}

function memoryCreateFrom(name: string, description: string, draft: WorkflowDef): Workflow {
  const created = memoryCreateWorkflow(name, description, null);
  const entry = requireEntry(created.id);
  entry.draft = clone(draft);
  entry.workflow.name = name;
  return clone(entry.workflow);
}

export function memoryResetExamples(): WorkflowSummary[] {
  entries = null;
  return memoryListWorkflows();
}

export interface WorkflowExportFile {
  format: string;
  name: string;
  description: string;
  definition: WorkflowDef;
}

export function memoryExportFile(id: string): WorkflowExportFile {
  const entry = requireEntry(id);
  return {
    format: "multara-flow/1",
    name: entry.workflow.name,
    description: entry.workflow.description,
    definition: clone(entry.draft),
  };
}

export function memoryImportText(raw: string): Workflow {
  let file: WorkflowExportFile;
  try {
    file = JSON.parse(raw) as WorkflowExportFile;
  } catch (cause) {
    throw new Error(cause instanceof Error ? cause.message : String(cause));
  }
  if (!file.format?.startsWith("multara-flow/") || !file.definition) {
    throw new Error(t("workflow.web.importInvalid"));
  }
  return memoryCreateFrom(file.name || t("workflow.list.blank"), file.description ?? "", file.definition);
}

export function memoryNodeTemplates(): NodeTemplate[] {
  return clone(templateStore());
}

export function memorySaveNodeTemplate(name: string, description: string, template: { nodes: NodeTemplate["nodes"]; edges: NodeTemplate["edges"] }): NodeTemplate[] {
  templateStore().push({
    id: `user.${crypto.randomUUID()}`,
    category: "user",
    name,
    description,
    recommends: null,
    nodes: clone(template.nodes),
    edges: clone(template.edges),
    builtin: false,
  });
  return memoryNodeTemplates();
}

export function memoryDeleteNodeTemplate(id: string): NodeTemplate[] {
  const list = templateStore();
  const index = list.findIndex((item) => item.id === id && !item.builtin);
  if (index >= 0) list.splice(index, 1);
  return memoryNodeTemplates();
}

function secretStore(): SecretName[] {
  if (!secrets) secrets = clone(fixtureSecrets());
  return secrets;
}

export function memorySecrets(): SecretName[] {
  return clone(secretStore());
}

export function memorySetSecret(name: string, _value: string, description = ""): SecretName[] {
  const list = secretStore();
  const existing = list.find((item) => item.name === name);
  if (existing) {
    existing.description = description;
    existing.updatedAt = now();
  } else {
    list.push({ name, description, updatedAt: now() });
  }
  return memorySecrets();
}

export function memoryDeleteSecret(name: string): SecretName[] {
  const list = secretStore();
  const index = list.findIndex((item) => item.name === name);
  if (index >= 0) list.splice(index, 1);
  return memorySecrets();
}

export function downloadJson(filename: string, value: unknown) {
  const blob = new Blob([JSON.stringify(value, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}

export function pickTextFile(): Promise<string | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "application/json,.json";
    input.onchange = () => {
      const file = input.files?.[0];
      if (!file) {
        resolve(null);
        return;
      }
      void file.text().then(resolve, () => resolve(null));
    };
    input.click();
  });
}
