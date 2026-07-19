import type {
  WorkflowDefinition,
  WorkflowEdge,
  WorkflowNode,
} from "@mcp_link/shared";

export const HOOK_METHODS = [
  "tools/list",
  "tools/call",
  "resources/list",
  "resources/read",
  "prompts/list",
  "prompts/get",
] as const;

export type HookMethod = (typeof HOOK_METHODS)[number];
export type HookTiming = "before" | "after";

export interface HookRule {
  id: string;
  name: string;
  enabled: boolean;
  method: HookMethod;
  timing: HookTiming;
  script: string;
  createdAt: number;
  updatedAt: number;
}

function isHookMethod(value: string): value is HookMethod {
  return HOOK_METHODS.includes(value as HookMethod);
}

function isHookRuleShape(workflow: WorkflowDefinition): boolean {
  if (workflow.nodes.length !== 4) return false;

  const start = findNode(workflow, "start");
  const hook = findNode(workflow, "hook");
  const mcp = findNode(workflow, "mcp-call");
  const end = findNode(workflow, "end");
  if (!start || !hook || !mcp || !end) return false;

  const before =
    hasEdge(workflow.edges, start.id, hook.id) &&
    hasEdge(workflow.edges, hook.id, mcp.id) &&
    hasEdge(workflow.edges, mcp.id, end.id);
  const after =
    hasEdge(workflow.edges, start.id, mcp.id) &&
    hasEdge(workflow.edges, mcp.id, hook.id) &&
    hasEdge(workflow.edges, hook.id, end.id);

  return before || after;
}

export function toHookRule(workflow: WorkflowDefinition): HookRule | null {
  const method = isHookMethod(workflow.workflowType)
    ? workflow.workflowType
    : "tools/call";

  if (!isHookRuleShape(workflow)) return null;

  const start = findNode(workflow, "start");
  const hook = findNode(workflow, "hook");
  const timing =
    start && hook && hasEdge(workflow.edges, start.id, hook.id)
      ? "before"
      : "after";

  return {
    id: workflow.id,
    name: workflow.name,
    enabled: workflow.enabled ?? false,
    method,
    timing,
    script: extractHookScript(hook),
    createdAt: workflow.createdAt ?? Date.now(),
    updatedAt: workflow.updatedAt ?? Date.now(),
  };
}

function toWorkflowDefinition(rule: HookRule): WorkflowDefinition {
  const startId = `start-${rule.id}`;
  const hookId = `hook-${rule.id}`;
  const mcpId = `mcp-${rule.id}`;
  const endId = `end-${rule.id}`;

  const nodes: WorkflowNode[] = [
    {
      id: startId,
      type: "start",
      position: { x: 0, y: 0 },
      data: { label: "Start" },
      deletable: false,
    },
    {
      id: hookId,
      type: "hook",
      position: { x: 200, y: 0 },
      data: {
        label: rule.name,
        hook: {
          id: hookId,
          script: rule.script,
          blocking: true,
        },
      },
    },
    {
      id: mcpId,
      type: "mcp-call",
      position: { x: 400, y: 0 },
      data: { label: "MCP Call" },
      deletable: false,
    },
    {
      id: endId,
      type: "end",
      position: { x: 600, y: 0 },
      data: { label: "End" },
      deletable: false,
    },
  ];

  const edges: WorkflowEdge[] =
    rule.timing === "before"
      ? [
          { id: `edge-${rule.id}-start-hook`, source: startId, target: hookId },
          { id: `edge-${rule.id}-hook-mcp`, source: hookId, target: mcpId },
          { id: `edge-${rule.id}-mcp-end`, source: mcpId, target: endId },
        ]
      : [
          { id: `edge-${rule.id}-start-mcp`, source: startId, target: mcpId },
          { id: `edge-${rule.id}-mcp-hook`, source: mcpId, target: hookId },
          { id: `edge-${rule.id}-hook-end`, source: hookId, target: endId },
        ];

  return {
    id: rule.id,
    name: rule.name,
    workflowType: rule.method,
    nodes,
    edges,
    enabled: rule.enabled,
    createdAt: rule.createdAt,
    updatedAt: Date.now(),
  };
}

export function toWorkflowCreateInput(rule: HookRule) {
  const {
    id: _id,
    createdAt: _createdAt,
    updatedAt: _updatedAt,
    ...input
  } = toWorkflowDefinition(rule);
  return input;
}

export function toWorkflowUpdateInput(rule: HookRule) {
  const {
    id: _id,
    createdAt: _createdAt,
    ...input
  } = toWorkflowDefinition(rule);
  return input;
}

function findNode(workflow: WorkflowDefinition, type: WorkflowNode["type"]) {
  return workflow.nodes.find((node) => node.type === type);
}

function hasEdge(edges: WorkflowEdge[], source: string, target: string) {
  return edges.some((edge) => edge.source === source && edge.target === target);
}

function extractHookScript(node?: WorkflowNode) {
  const hook = node?.data?.hook;
  if (hook?.script) return hook.script;
  const inlineScript = node?.data?.script;
  return typeof inlineScript === "string" ? inlineScript : "";
}
