import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PromptBox } from "@/components/PromptBox";

// ── Mocks ────────────────────────────────────────────────────────────────

// Use mutable state so individual tests can change the mock return
let playgroundState: Record<string, unknown> = {
  prompt: "",
  selectedModel: "llama-3",
  messages: [],
  isGenerating: false,
  setPrompt: vi.fn(),
};

let playgroundV2State: Record<string, unknown> = {
  activeConversationId: null,
  conversations: {},
  isGenerating: false,
};

vi.mock("@/lib/stores/playground", () => ({
  usePlaygroundStore: (selector: (s: Record<string, unknown>) => unknown) =>
    selector(playgroundState),
}));

vi.mock("@/lib/stores/playground-v2", () => ({
  usePlaygroundV2Store: (selector: (s: Record<string, unknown>) => unknown) =>
    selector(playgroundV2State),
}));

// Mock rate limit hook
vi.mock("@/lib/hooks/use-rate-limit", () => ({
  useRateLimit: () => ({
    hasData: false,
    isLimited: false,
    isNearLimit: false,
    requestLimit: undefined,
    requestRemaining: undefined,
    resetTimestamp: undefined,
    tokenLimit: undefined,
    tokenRemaining: undefined,
    percentage: undefined,
    secondsUntilReset: 0,
    updateFromResponse: vi.fn(),
  }),
}));

describe("PromptBox", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    playgroundState = {
      prompt: "",
      selectedModel: "llama-3",
      messages: [],
      isGenerating: false,
      setPrompt: vi.fn(),
    };
    playgroundV2State = {
      activeConversationId: null,
      conversations: {},
      isGenerating: false,
    };
  });

  it("renders a textarea", () => {
    render(<PromptBox />);
    expect(screen.getByPlaceholderText("Ask anything...")).toBeInTheDocument();
  });

  it("renders a send button", () => {
    render(<PromptBox />);
    const sendBtn = screen.getByLabelText("Send message");
    expect(sendBtn).toBeInTheDocument();
  });

  it("send button is disabled when prompt is empty", () => {
    render(<PromptBox />);
    const sendBtn = screen.getByLabelText("Send message");
    expect(sendBtn).toBeDisabled();
  });

  it("calls setPrompt when typing in the textarea", async () => {
    const user = userEvent.setup();
    render(<PromptBox />);
    const textarea = screen.getByPlaceholderText("Ask anything...") as HTMLTextAreaElement;
    await user.type(textarea, "Hello");
    expect(playgroundState.setPrompt as ReturnType<typeof vi.fn>).toHaveBeenCalled();
  });

  it("calls onSubmit when the send button is clicked", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    playgroundState.prompt = "Hello world";

    render(<PromptBox onSubmit={onSubmit} />);
    const sendBtn = screen.getByLabelText("Send message");
    expect(sendBtn).toBeEnabled();
    await user.click(sendBtn);
    expect(onSubmit).toHaveBeenCalledOnce();
  });

  it("calls onSubmit when Enter is pressed in the textarea", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    playgroundState.prompt = "Test prompt";

    render(<PromptBox onSubmit={onSubmit} />);
    const textarea = screen.getByPlaceholderText("Ask anything...");
    await user.type(textarea, "{Enter}");
    expect(onSubmit).toHaveBeenCalledOnce();
  });

  it("shows 'Continue the conversation...' placeholder when there are messages", () => {
    playgroundV2State = {
      activeConversationId: "convo-1",
      conversations: {
        "convo-1": {
          id: "convo-1",
          title: "Chat",
          model: "llama-3",
          messages: [{ id: "m1", role: "user", content: "Hi", timestamp: 1 }],
          createdAt: 1,
          updatedAt: 1,
          totalTokens: 0,
        },
      },
      isGenerating: false,
    };

    render(<PromptBox />);
    expect(screen.getByPlaceholderText("Continue the conversation...")).toBeInTheDocument();
  });

  it("is disabled when generating", () => {
    playgroundState.isGenerating = true;
    playgroundState.prompt = "Hello";

    render(<PromptBox />);
    const textarea = screen.getByPlaceholderText("Ask anything...") as HTMLTextAreaElement;
    expect(textarea).toBeDisabled();
  });
});
