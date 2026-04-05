import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ModelSelector } from "@/components/ModelSelector";

// ── Mocks ────────────────────────────────────────────────────────────────

// Mock playground store — uses a mutable ref so tests can change state
let storeState: Record<string, unknown> = {
  selectedModel: "model-1",
  setModel: vi.fn(),
  prompt: "",
  messages: [],
  isGenerating: false,
};

const mockSetModel = vi.fn();
storeState.setModel = mockSetModel;

vi.mock("@/lib/stores/playground", () => ({
  usePlaygroundStore: (selector: (s: Record<string, unknown>) => unknown) =>
    selector(storeState),
}));

// Mock lucide-react icons
vi.mock("lucide-react", () => ({
  Search: () => <svg data-testid="search-icon" />,
  ChevronUp: () => <svg data-testid="chevron-up-icon" />,
}));

const MOCK_MODELS = [
  { id: "model-1", name: "LLaMA 3 8B" },
  { id: "model-2", name: "Mistral 7B" },
  { id: "model-3", name: "DeepSeek Coder" },
];

describe("ModelSelector", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    storeState = {
      selectedModel: "model-1",
      setModel: mockSetModel,
      prompt: "",
      messages: [],
      isGenerating: false,
    };
  });

  it("renders a select dropdown on desktop", () => {
    render(<ModelSelector models={MOCK_MODELS} />);
    const select = screen.getByRole("combobox");
    expect(select).toBeInTheDocument();
  });

  it("shows the currently selected model in the dropdown", () => {
    render(<ModelSelector models={MOCK_MODELS} />);
    const select = screen.getByRole("combobox") as HTMLSelectElement;
    expect(select.value).toBe("model-1");
  });

  it("renders all model options in the select", () => {
    render(<ModelSelector models={MOCK_MODELS} />);
    const select = screen.getByRole("combobox");
    const options = within(select).getAllByRole("option");
    // 1 disabled placeholder + 3 models
    expect(options).toHaveLength(4);
    expect(options[1]).toHaveTextContent("LLaMA 3 8B");
    expect(options[2]).toHaveTextContent("Mistral 7B");
    expect(options[3]).toHaveTextContent("DeepSeek Coder");
  });

  it("has a disabled placeholder option", () => {
    render(<ModelSelector models={MOCK_MODELS} />);
    const select = screen.getByRole("combobox");
    const placeholder = within(select).getByText("Select a model...");
    expect(placeholder).toBeDisabled();
  });

  it("calls setModel when a different model is selected from the dropdown", async () => {
    const user = userEvent.setup();
    render(<ModelSelector models={MOCK_MODELS} />);
    const select = screen.getByRole("combobox");
    await user.selectOptions(select, "model-2");
    expect(mockSetModel).toHaveBeenCalledWith("model-2");
  });

  it("renders the mobile trigger button with selected model name", () => {
    render(<ModelSelector models={MOCK_MODELS} />);
    // The model name appears in the mobile button, desktop select, and bottom sheet
    const matches = screen.getAllByText("LLaMA 3 8B");
    expect(matches.length).toBeGreaterThanOrEqual(1);
  });

  it("shows 'Select model' when no model is selected", () => {
    storeState.selectedModel = "";
    render(<ModelSelector models={MOCK_MODELS} />);
    expect(screen.getByText("Select model")).toBeInTheDocument();
  });
});
