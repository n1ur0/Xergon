import { NextRequest, NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type OperationType = "base" | "fine_tune" | "merge" | "prune" | "quantize";

interface LineageNodeData {
  id: string;
  name: string;
  version: string;
  operation: OperationType;
  createdAt: string;
  parentId?: string;
  parentIds?: string[];
  description?: string;
  parameters?: Record<string, string | number | boolean>;
}

interface LineageEdge {
  from: string;
  to: string;
  type: OperationType;
}

interface LineageTree {
  nodes: LineageNodeData[];
  edges: LineageEdge[];
}

// ---------------------------------------------------------------------------
// Mock data for llama-3.1-70b lineage
// ---------------------------------------------------------------------------

function generateLineageTree(modelId: string): LineageTree {
  // Default mock lineage for any model
  const baseId = `${modelId}-base`;
  const ft1Id = `${modelId}-ft-v1`;
  const ft2Id = `${modelId}-ft-v2`;
  const quant1Id = `${modelId}-quant-q4`;
  const quant2Id = `${modelId}-quant-q8`;
  const mergeId = `${modelId}-merge-v3`;
  const pruneId = `${modelId}-prune-small`;

  const nodes: LineageNodeData[] = [
    {
      id: baseId,
      name: modelId,
      version: "1.0.0",
      operation: "base",
      createdAt: "2024-07-01T00:00:00Z",
      description: "Original base model released by the model creator. Pre-trained on a diverse corpus of text data.",
      parameters: { "context_window": 128000, "hidden_size": 8192, "num_layers": 80, "precision": "bf16" },
    },
    {
      id: ft1Id,
      name: `${modelId}-instruct`,
      version: "1.1.0",
      operation: "fine_tune",
      createdAt: "2024-09-15T00:00:00Z",
      parentId: baseId,
      parentIds: [baseId],
      description: "Instruction-tuned variant using a curated dataset of instruction-response pairs for improved chat capabilities.",
      parameters: { "context_window": 128000, "training_data": "instruct-v1-50k", "epochs": 3, "learning_rate": 2e-5 },
    },
    {
      id: ft2Id,
      name: `${modelId}-code`,
      version: "1.2.0",
      operation: "fine_tune",
      createdAt: "2025-01-10T00:00:00Z",
      parentId: ft1Id,
      parentIds: [ft1Id],
      description: "Code-specialized fine-tune using a large corpus of code from multiple programming languages.",
      parameters: { "context_window": 128000, "training_data": "code-v2-200k", "epochs": 5, "learning_rate": 1e-5 },
    },
    {
      id: quant1Id,
      name: `${modelId}-instruct-q4`,
      version: "1.1.0-q4",
      operation: "quantize",
      createdAt: "2024-10-01T00:00:00Z",
      parentId: ft1Id,
      parentIds: [ft1Id],
      description: "4-bit quantized version of the instruct model using GPTQ quantization for efficient inference.",
      parameters: { "bits": 4, "group_size": 128, "quant_method": "gptq", "dataset": "c4" },
    },
    {
      id: quant2Id,
      name: `${modelId}-instruct-q8`,
      version: "1.1.0-q8",
      operation: "quantize",
      createdAt: "2024-10-05T00:00:00Z",
      parentId: ft1Id,
      parentIds: [ft1Id],
      description: "8-bit quantized version of the instruct model. Good balance between size and quality.",
      parameters: { "bits": 8, "group_size": 32, "quant_method": "awq", "dataset": "wikitext" },
    },
    {
      id: mergeId,
      name: `${modelId}-unified-v3`,
      version: "3.0.0",
      operation: "merge",
      createdAt: "2025-03-20T00:00:00Z",
      parentId: ft2Id,
      parentIds: [ft1Id, ft2Id],
      description: "Merged model combining instruction-tuning and code capabilities using SLERP merging for improved multi-task performance.",
      parameters: { "merge_method": "slerp", "t": 0.5, "base_model": ft1Id, "target_model": ft2Id },
    },
    {
      id: pruneId,
      name: `${modelId}-small`,
      version: "1.0.0-pruned",
      operation: "prune",
      createdAt: "2025-02-15T00:00:00Z",
      parentId: baseId,
      parentIds: [baseId],
      description: "Pruned version of the base model using magnitude pruning to reduce parameters by 30% while maintaining 95% of original performance.",
      parameters: { "sparsity": 0.3, "pruning_method": "magnitude", "layers_removed": 24, "final_layers": 56 },
    },
  ];

  const edges: LineageEdge[] = [
    { from: baseId, to: ft1Id, type: "fine_tune" },
    { from: ft1Id, to: ft2Id, type: "fine_tune" },
    { from: ft1Id, to: quant1Id, type: "quantize" },
    { from: ft1Id, to: quant2Id, type: "quantize" },
    { from: ft1Id, to: mergeId, type: "merge" },
    { from: ft2Id, to: mergeId, type: "merge" },
    { from: baseId, to: pruneId, type: "prune" },
  ];

  return { nodes, edges };
}

function getAncestors(tree: LineageTree, nodeId: string): LineageNodeData[] {
  const nodeMap = new Map(tree.nodes.map((n) => [n.id, n]));
  const visited = new Set<string>();
  const result: LineageNodeData[] = [];

  function walk(id: string) {
    const node = nodeMap.get(id);
    if (!node || visited.has(id)) return;
    visited.add(id);
    if (node.parentIds) {
      for (const pid of node.parentIds) {
        walk(pid);
      }
    }
    if (id !== nodeId) {
      result.push(node);
    }
  }

  walk(nodeId);
  return result;
}

function getDescendants(tree: LineageTree, nodeId: string): LineageNodeData[] {
  const childrenMap = new Map<string, string[]>();
  for (const edge of tree.edges) {
    const existing = childrenMap.get(edge.from) ?? [];
    existing.push(edge.to);
    childrenMap.set(edge.from, existing);
  }

  const nodeMap = new Map(tree.nodes.map((n) => [n.id, n]));
  const visited = new Set<string>();
  const result: LineageNodeData[] = [];

  function walk(id: string) {
    const children = childrenMap.get(id) ?? [];
    for (const cid of children) {
      if (visited.has(cid)) continue;
      visited.add(cid);
      const node = nodeMap.get(cid);
      if (node) result.push(node);
      walk(cid);
    }
  }

  walk(nodeId);
  return result;
}

// ---------------------------------------------------------------------------
// GET /api/lineage
// ---------------------------------------------------------------------------

export async function GET(request: NextRequest) {
  try {
    const { searchParams } = new URL(request.url);
    const modelId = searchParams.get("modelId");
    const section = searchParams.get("section"); // ancestors | descendants

    if (!modelId) {
      return NextResponse.json(
        { error: "Missing modelId parameter" },
        { status: 400 },
      );
    }

    const tree = generateLineageTree(modelId);

    if (section === "ancestors") {
      return NextResponse.json(getAncestors(tree, modelId));
    }

    if (section === "descendants") {
      return NextResponse.json(getDescendants(tree, modelId));
    }

    // Return full tree (default: find root or return the model's tree)
    return NextResponse.json({ ...tree, degraded: true });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Internal server error" },
      { status: 500 },
    );
  }
}

// ---------------------------------------------------------------------------
// POST /api/lineage
// ---------------------------------------------------------------------------

export async function POST(request: NextRequest) {
  try {
    const body = await request.json();
    const { action, modelId, parentIds, operation, description, parameters } = body;

    if (action === "record") {
      if (!modelId || !operation) {
        return NextResponse.json(
          { error: "Missing modelId or operation" },
          { status: 400 },
        );
      }

      if (!["fine_tune", "merge", "prune", "quantize"].includes(operation)) {
        return NextResponse.json(
          { error: "Invalid operation type" },
          { status: 400 },
        );
      }

      // In production this would record the lineage event on-chain
      return NextResponse.json({
        success: true,
        recorded: {
          modelId,
          parentIds: parentIds ?? [],
          operation,
          description: description ?? "",
          parameters: parameters ?? {},
          createdAt: new Date().toISOString(),
        },
      });
    }

    return NextResponse.json({ error: "Invalid action" }, { status: 400 });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Internal server error" },
      { status: 500 },
    );
  }
}
