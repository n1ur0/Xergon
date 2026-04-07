import { NextRequest, NextResponse } from "next/server";

export async function POST(request: NextRequest) {
  const start = performance.now();

  const body = await request.json();
  const { endpoint, ...payload } = body;

  // Simulate processing delay
  await new Promise((resolve) => setTimeout(resolve, 200 + Math.random() * 300));

  const elapsed = Math.round(performance.now() - start);

  let response: Record<string, unknown>;
  let statusCode = 200;

  try {
    switch (endpoint) {
      case "/v1/chat/completions": {
        const model = payload.model || "llama-3.1-8b";
        const messages = payload.messages || [];
        const stream = payload.stream || false;

        if (stream) {
          response = {
            id: `chatcmpl-${Math.random().toString(36).slice(2, 10)}`,
            object: "chat.completion.chunk",
            created: Math.floor(Date.now() / 1000),
            model,
            choices: [
              {
                index: 0,
                delta: { role: "assistant", content: "Hello! This is a mock streamed response from Xergon Relay." },
                finish_reason: "stop",
              },
            ],
          };
        } else {
          const lastMsg = messages[messages.length - 1];
          const userContent = lastMsg?.content || "No input";
          response = {
            id: `chatcmpl-${Math.random().toString(36).slice(2, 10)}`,
            object: "chat.completion",
            created: Math.floor(Date.now() / 1000),
            model,
            choices: [
              {
                index: 0,
                message: {
                  role: "assistant",
                  content: `[Mock response] You said: "${userContent}". This is a simulated response from the Xergon API playground. In production, this would contain a real AI-generated response powered by decentralized GPU providers on the Ergo blockchain.`,
                },
                finish_reason: "stop",
              },
            ],
            usage: {
              prompt_tokens: Math.max(1, messages.length * 8),
              completion_tokens: 42,
              total_tokens: Math.max(1, messages.length * 8) + 42,
            },
          };
        }
        break;
      }

      case "/v1/completions": {
        const model = payload.model || "llama-3.1-8b";
        response = {
          id: `cmpl-${Math.random().toString(36).slice(2, 10)}`,
          object: "text_completion",
          created: Math.floor(Date.now() / 1000),
          model,
          choices: [
            {
              text: "[Mock completion] This is a simulated text completion from Xergon Relay.",
              index: 0,
              finish_reason: "stop",
            },
          ],
          usage: { prompt_tokens: 10, completion_tokens: 12, total_tokens: 22 },
        };
        break;
      }

      case "/v1/embeddings": {
        const model = payload.model || "llama-3.1-8b";
        const inputTexts = Array.isArray(payload.input) ? payload.input : [payload.input || "test"];
        response = {
          object: "list",
          data: inputTexts.map((_: string, i: number) => ({
            object: "embedding",
            embedding: Array.from({ length: 8 }, () => Math.random()),
            index: i,
          })),
          model,
          usage: {
            prompt_tokens: inputTexts.length * 5,
            total_tokens: inputTexts.length * 5,
          },
        };
        break;
      }

      case "/v1/models": {
        response = {
          object: "list",
          data: [
            { id: "llama-3.1-8b", object: "model", created: 1714000000, owned_by: "meta" },
            { id: "llama-3.1-70b", object: "model", created: 1714000000, owned_by: "meta" },
            { id: "mixtral-8x7b", object: "model", created: 1713000000, owned_by: "mistral" },
            { id: "deepseek-coder-v2", object: "model", created: 1715000000, owned_by: "deepseek" },
            { id: "qwen-2.5-72b", object: "model", created: 1716000000, owned_by: "alibaba" },
            { id: "mistral-7b", object: "model", created: 1712000000, owned_by: "mistral" },
            { id: "phi-4", object: "model", created: 1717000000, owned_by: "microsoft" },
            { id: "llama-3.1-405b", object: "model", created: 1714000000, owned_by: "meta" },
            { id: "command-r-plus", object: "model", created: 1713500000, owned_by: "cohere" },
          ],
        };
        break;
      }

      default: {
        statusCode = 404;
        response = {
          error: {
            message: `Unknown endpoint: ${endpoint}`,
            type: "invalid_request_error",
            code: "not_found",
          },
        };
      }
    }
  } catch {
    statusCode = 400;
    response = {
      error: {
        message: "Invalid request body",
        type: "invalid_request_error",
        code: "invalid_json",
      },
    };
  }

  return NextResponse.json(
    { ...response, _meta: { responseTime: elapsed } },
    { status: statusCode }
  );
}
