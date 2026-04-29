import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { searchDocs } from "./tools/search_docs.ts";
import { getConcept, getErgoscriptRef, getPattern, getCookbook, getSkill } from "./tools/concepts.ts";
import { getEip } from "./tools/get_eip.ts";
import { listProjects } from "./tools/list_projects.ts";
import { getEntity } from "./tools/get_entity.ts";

const server = new Server(
  {
    name: "ergo-kb-docs",
    version: "0.2.0",
  },
  {
    capabilities: {
      tools: {},
    },
  }
);

// ─── Tool definitions ─────────────────────────────────────────────────────────

server.setRequestHandler(ListToolsRequestSchema, () => {
  return {
    tools: [
      {
        name: "search_docs",
        description:
          "Full-text BM25 search across all ergodocs, wiki, and wiki-raw sources. " +
          "Returns ranked results with title, category, source, and summary.",
        inputSchema: {
          type: "object",
          properties: {
            query: {
              type: "string",
              description: "Search query string",
            },
            limit: {
              type: "number",
              description: "Max results to return (default 5)",
            },
          },
          required: ["query"],
        },
      },
      {
        name: "get_concept",
        description:
          "Look up an Ergo concept (eUTXO, Sigma protocols, storage rent, etc.) by name. " +
          "See also: get_ergoscript_ref for language refs, get_pattern for contract patterns.",
        inputSchema: {
          type: "object",
          properties: {
            name: {
              type: "string",
              description:
                "Concept name. Examples: eUTXO, sigma protocols, storage rent, " +
                "boxes and registers, oracle pools, babel fees, ergoauth, ergopay, " +
                "eip-12, nautilus, fleet sdk, sigma rust, ergoscript",
            },
          },
          required: ["name"],
        },
      },
      {
        name: "get_eip",
        description:
          "Look up an Ergo Improvement Proposal by number (EIP-1 through EIP-44). " +
          "Covers token standards, oracle pools, Babel fees, multisig, wallets, and more.",
        inputSchema: {
          type: "object",
          properties: {
            number: {
              type: "number",
              description: "EIP number (e.g. 4, 12, 20, 21, 31)",
            },
          },
          required: ["number"],
        },
      },
      {
        name: "get_ergoscript_ref",
        description:
          "Look up an ErgoScript language reference topic. " +
          "Examples: context-variables, types, functions, box, syntax, compiler.",
        inputSchema: {
          type: "object",
          properties: {
            topic: {
              type: "string",
              description: "ErgoScript ref topic name",
            },
          },
          required: ["topic"],
        },
      },
      {
        name: "get_pattern",
        description:
          "Look up an Ergo smart-contract design pattern by name. " +
          "Examples: stealth-address, schnorr-verification, bulletproof, trustless-peg.",
        inputSchema: {
          type: "object",
          properties: {
            name: {
              type: "string",
              description: "Pattern name",
            },
          },
          required: ["name"],
        },
      },
      {
        name: "get_cookbook",
        description:
          "Look up a code recipe (Fleet SDK, tokens, NFT, oracle, DEX patterns).",
        inputSchema: {
          type: "object",
          properties: {
            recipe: {
              type: "string",
              description: "Recipe name",
            },
          },
          required: ["recipe"],
        },
      },
      {
        name: "list_projects",
        description:
          "List all ecosystem projects from ergodocs:eco/ and wiki:entities/. " +
          "Returns project name, source, category, and summary for the first 200.",
        inputSchema: {
          type: "object",
          properties: {},
        },
      },
      {
        name: "get_entity",
        description:
          "Look up any ecosystem project or wiki entity by name. Searches both " +
          "ergodocs:eco/ and wiki:entities/ with fuzzy matching. Returns full details " +
          "including summary, tags, headings, and full content. Also suggests close " +
          "alternatives if the name is slightly off.",
        inputSchema: {
          type: "object",
          properties: {
            name: {
              type: "string",
              description:
                "Entity name or slug. Examples: ergo-mcp, Dexy Peg Bots, " +
                "xergon, yolo-chain, degens-world, rust-expert-mcp, agent-army",
            },
          },
          required: ["name"],
        },
      },
      {
        name: "get_skill",
        description:
          "Look up a skill guide (Nautilus, Babel fees, Fleet SDK, AppKit, Sigma Rust).",
        inputSchema: {
          type: "object",
          properties: {
            name: {
              type: "string",
              description: "Skill name",
            },
          },
          required: ["name"],
        },
      },
    ],
  };
});

// ─── Tool handler ─────────────────────────────────────────────────────────────

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;
  const argObj = args as Record<string, unknown>;

  try {
    let result: string;

    switch (name) {
      case "search_docs":
        result = JSON.stringify(
          await searchDocs(
            String(argObj.query ?? ""),
            Number(argObj.limit ?? 5)
          )
        );
        break;

      case "get_concept":
        result = await getConcept(String(argObj.name ?? ""));
        break;

      case "get_eip":
        result = await getEip(argObj.number);
        break;

      case "get_ergoscript_ref":
        result = await getErgoscriptRef(String(argObj.topic ?? ""));
        break;

      case "get_pattern":
        result = await getPattern(String(argObj.name ?? ""));
        break;

      case "get_cookbook":
        result = await getCookbook(String(argObj.recipe ?? ""));
        break;

      case "list_projects":
        result = await listProjects();
        break;

      case "get_entity":
        result = await getEntity(String(argObj.name ?? ""));
        break;

      case "get_skill":
        result = await getSkill(String(argObj.name ?? ""));
        break;

      default:
        return {
          content: [
            {
              type: "text",
              text: JSON.stringify({ error: `Unknown tool: ${name}` }),
            },
          ],
          isError: true,
        };
    }

    return {
      content: [{ type: "text", text: result }],
    };
  } catch (err) {
    return {
      content: [
        {
          type: "text",
          text: JSON.stringify({ error: String(err) }),
        },
      ],
      isError: true,
    };
  }
});

// ─── Start ─────────────────────────────────────────────────────────────────────

const transport = new StdioServerTransport();
server.connect(transport);
