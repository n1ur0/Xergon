import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { ResponseArea } from "@/components/ResponseArea";

// ── Mocks ────────────────────────────────────────────────────────────────

const mockMessages = vi.fn();

vi.mock("@/lib/stores/playground", () => ({
  usePlaygroundStore: (selector: (s: Record<string, unknown>) => unknown) =>
    selector({
      messages: mockMessages(),
      isGenerating: false,
    }),
}));

// Mock playground v2 store
vi.mock("@/lib/stores/playground-v2", () => ({
  usePlaygroundV2Store: (selector: (s: Record<string, unknown>) => unknown) =>
    selector({
      activeConversationId: null,
      conversations: {},
      isGenerating: false,
    }),
}));

// Mock TokenCounter
vi.mock("@/components/playground/TokenCounter", () => ({
  TokenCounter: () => <span data-testid="token-counter">0 tokens</span>,
}));

// Mock highlight.js CSS
vi.mock("highlight.js/styles/github-dark.min.css", () => ({}));

// Mock clipboard API
Object.assign(navigator, {
  clipboard: {
    writeText: vi.fn().mockResolvedValue(undefined),
  },
});

describe("ResponseArea", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows placeholder text when there are no messages", () => {
    mockMessages.mockReturnValue([]);
    render(<ResponseArea />);
    expect(screen.getByText("Responses will appear here.")).toBeInTheDocument();
  });

  it("renders user message content", () => {
    mockMessages.mockReturnValue([
      {
        id: "msg-1",
        role: "user",
        content: "Hello, how are you?",
        timestamp: Date.now(),
      },
    ]);
    render(<ResponseArea />);
    expect(screen.getByText("Hello, how are you?")).toBeInTheDocument();
  });

  it("renders assistant message content as markdown", () => {
    mockMessages.mockReturnValue([
      {
        id: "msg-2",
        role: "assistant",
        content: "**Bold text** and *italic text*",
        model: "llama-3",
        timestamp: Date.now(),
        inputTokens: 10,
        outputTokens: 20,
      },
    ]);
    render(<ResponseArea />);
    expect(screen.getByText("Bold text")).toBeInTheDocument();
    expect(screen.getByText("italic text")).toBeInTheDocument();
  });

  it("renders the model name for assistant messages", () => {
    mockMessages.mockReturnValue([
      {
        id: "msg-3",
        role: "assistant",
        content: "Some response",
        model: "mistral-7b",
        timestamp: Date.now(),
      },
    ]);
    render(<ResponseArea />);
    expect(screen.getByText(/mistral-7b/)).toBeInTheDocument();
  });

  it("renders code blocks in assistant messages", () => {
    mockMessages.mockReturnValue([
      {
        id: "msg-4",
        role: "assistant",
        content: "Here is some code:\n\n```javascript\nconst x = 42;\n```\n\nDone.",
        model: "llama-3",
        timestamp: Date.now(),
      },
    ]);
    render(<ResponseArea />);
    // highlight.js wraps tokens in spans, so text is split.
    // Use a custom text matcher to find elements containing the code text.
    const codeBlocks = document.querySelectorAll("pre code");
    expect(codeBlocks.length).toBe(1);
    // Check that the code block contains the keywords (they may be in spans)
    expect(codeBlocks[0].textContent).toContain("const");
    expect(codeBlocks[0].textContent).toContain("x");
    expect(codeBlocks[0].textContent).toContain("42");
  });

  it("shows a copy button for assistant messages with content", () => {
    mockMessages.mockReturnValue([
      {
        id: "msg-5",
        role: "assistant",
        content: "Copy me",
        model: "llama-3",
        timestamp: Date.now(),
      },
    ]);
    render(<ResponseArea />);
    const copyBtn = screen.getByTitle("Copy response");
    expect(copyBtn).toBeInTheDocument();
  });

  it("renders multiple messages in order", () => {
    mockMessages.mockReturnValue([
      { id: "m1", role: "user", content: "First", timestamp: 1 },
      { id: "m2", role: "assistant", content: "Second", model: "m", timestamp: 2 },
      { id: "m3", role: "user", content: "Third", timestamp: 3 },
    ]);
    render(<ResponseArea />);
    expect(screen.getByText("First")).toBeInTheDocument();
    expect(screen.getByText("Second")).toBeInTheDocument();
    expect(screen.getByText("Third")).toBeInTheDocument();
  });

  it("renders inline code in assistant messages", () => {
    mockMessages.mockReturnValue([
      {
        id: "msg-6",
        role: "assistant",
        content: "Use the `useState` hook.",
        model: "llama-3",
        timestamp: Date.now(),
      },
    ]);
    render(<ResponseArea />);
    expect(screen.getByText("useState")).toBeInTheDocument();
  });
});
