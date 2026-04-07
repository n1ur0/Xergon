/**
 * Flow/Pipeline Builder
 *
 * Provides a step-based pipeline system for composing multi-step
 * LLM workflows. Includes built-in flows for common patterns like
 * chain-of-thought, self-critique, and multi-review.
 */

// ── Types ─────────────────────────────────────────────────────────

export interface FlowStep {
  name: string;
  model?: string;
  systemPrompt?: string;
  transform?: (input: string) => string | Promise<string>;
  condition?: (input: string) => boolean;
}

export interface Flow {
  name: string;
  description: string;
  steps: FlowStep[];
}

export interface FlowResult {
  stepResults: Array<{ step: string; output: string; duration: number }>;
  finalOutput: string;
  totalDuration: number;
}

export type FlowExecutor = (
  model: string,
  messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }>,
) => Promise<{ content: string }>;

// ── Built-in Flows ────────────────────────────────────────────────

const BUILT_IN_FLOWS: Flow[] = [
  {
    name: 'chain-of-thought',
    description: 'Think step by step, then provide the final answer',
    steps: [
      {
        name: 'think',
        systemPrompt: 'You are a careful thinker. Break down the problem step by step. Show your reasoning clearly.',
      },
      {
        name: 'answer',
        systemPrompt: 'Based on the reasoning above, provide a clear, concise final answer.',
        transform: (input: string) => `Here is the step-by-step reasoning:\n\n${input}\n\nNow provide the final answer:`,
      },
    ],
  },
  {
    name: 'self-critique',
    description: 'Generate a response, critique it, then produce an improved version',
    steps: [
      {
        name: 'draft',
        systemPrompt: 'Provide a thorough initial response to the following request.',
      },
      {
        name: 'critique',
        systemPrompt: 'You are a critical reviewer. Analyze the following response. Identify weaknesses, errors, areas for improvement, and missing considerations. Be specific and constructive.',
        transform: (input: string) => `Original request:\n\nPlease review this response:\n\n${input}`,
      },
      {
        name: 'improve',
        systemPrompt: 'Based on the original draft and the critique, produce an improved, polished response that addresses all criticisms.',
        transform: (_input: string) => {
          // This step receives the critique output; the transform is handled by runFlow
          return _input;
        },
      },
    ],
  },
  {
    name: 'multi-review',
    description: '3 reviewers with different perspectives, then synthesize',
    steps: [
      {
        name: 'reviewer-optimist',
        systemPrompt: 'You are an optimistic reviewer. Focus on strengths, opportunities, and what works well. Provide positive feedback.',
      },
      {
        name: 'reviewer-critic',
        systemPrompt: 'You are a critical reviewer. Focus on weaknesses, risks, and what needs improvement. Be thorough and specific.',
      },
      {
        name: 'reviewer-pragmatist',
        systemPrompt: 'You are a pragmatic reviewer. Focus on practical considerations, feasibility, and trade-offs. Be balanced and realistic.',
      },
      {
        name: 'synthesize',
        systemPrompt: 'Synthesize the three review perspectives below into a balanced, comprehensive assessment with actionable recommendations.',
        transform: (_input: string) => _input,
      },
    ],
  },
  {
    name: 'code-review',
    description: 'Analyze code, review for issues, and suggest improvements',
    steps: [
      {
        name: 'analyze',
        systemPrompt: 'You are a code analyst. Describe what this code does, its structure, and its purpose. Be concise.',
      },
      {
        name: 'review',
        systemPrompt: 'You are a senior code reviewer. Review this code for bugs, security issues, performance problems, style issues, and best practices. Rate severity of each issue.',
        transform: (input: string) => `Code Analysis:\n${input}\n\nNow review this code thoroughly:`,
      },
      {
        name: 'improve',
        systemPrompt: 'Based on the review findings, suggest specific improvements. Provide corrected code snippets where applicable. Prioritize by impact.',
        transform: (_input: string) => _input,
      },
    ],
  },
  {
    name: 'summarize-cascade',
    description: 'Summarize in 3 decreasing lengths: detailed, medium, brief',
    steps: [
      {
        name: 'detailed-summary',
        systemPrompt: 'Provide a detailed summary of the following text. Cover all key points, arguments, and conclusions. Aim for ~500 words.',
      },
      {
        name: 'medium-summary',
        systemPrompt: 'Provide a medium-length summary of the following detailed summary. Focus on the most important points. Aim for ~200 words.',
        transform: (input: string) => `Detailed summary:\n\n${input}\n\nNow create a medium summary:`,
      },
      {
        name: 'brief-summary',
        systemPrompt: 'Provide a brief, concise summary in 2-3 sentences. Capture only the essential message.',
        transform: (input: string) => `Medium summary:\n\n${input}\n\nNow create a brief summary:`,
      },
    ],
  },
];

// ── API ───────────────────────────────────────────────────────────

/**
 * Create a custom flow from steps.
 */
export function createFlow(
  name: string,
  description: string,
  steps: FlowStep[],
): Flow {
  return { name, description, steps };
}

/**
 * Run a flow sequentially. Each step's output feeds into the next step's input.
 * Requires an executor function that calls the LLM API.
 */
export async function runFlow(
  flow: Flow,
  input: string,
  executor: FlowExecutor,
  defaultModel: string = 'llama-3.3-70b',
): Promise<FlowResult> {
  const startTime = Date.now();
  const stepResults: FlowResult['stepResults'] = [];
  let currentInput = input;

  for (const step of flow.steps) {
    // Check condition
    if (step.condition && !step.condition(currentInput)) {
      stepResults.push({ step: step.name, output: '(skipped - condition not met)', duration: 0 });
      continue;
    }

    const stepStart = Date.now();

    // Apply transform if present
    let stepInput = currentInput;
    if (step.transform) {
      stepInput = await step.transform(currentInput);
    }

    // Build messages for this step
    const messages: Array<{ role: 'system' | 'user'; content: string }> = [];
    if (step.systemPrompt) {
      messages.push({ role: 'system', content: step.systemPrompt });
    }
    messages.push({ role: 'user', content: stepInput });

    // Execute the step
    const result = await executor(step.model ?? defaultModel, messages);
    currentInput = result.content;

    const stepDuration = Date.now() - stepStart;
    stepResults.push({ step: step.name, output: result.content, duration: stepDuration });
  }

  return {
    stepResults,
    finalOutput: currentInput,
    totalDuration: Date.now() - startTime,
  };
}

/**
 * Run independent steps in parallel. Steps are considered independent
 * if they all receive the original input (no chaining).
 * Steps with `condition: false` are skipped.
 */
export async function runFlowParallel(
  flow: Flow,
  input: string,
  executor: FlowExecutor,
  defaultModel: string = 'llama-3.3-70b',
): Promise<FlowResult> {
  const startTime = Date.now();
  const stepResults: FlowResult['stepResults'] = [];

  const promises = flow.steps.map(async (step) => {
    const stepStart = Date.now();

    // Check condition
    if (step.condition && !step.condition(input)) {
      return { step: step.name, output: '(skipped - condition not met)', duration: 0 } as const;
    }

    let stepInput = input;
    if (step.transform) {
      stepInput = await step.transform(input);
    }

    const messages: Array<{ role: 'system' | 'user'; content: string }> = [];
    if (step.systemPrompt) {
      messages.push({ role: 'system', content: step.systemPrompt });
    }
    messages.push({ role: 'user', content: stepInput });

    const result = await executor(step.model ?? defaultModel, messages);

    return {
      step: step.name,
      output: result.content,
      duration: Date.now() - stepStart,
    };
  });

  const results = await Promise.all(promises);
  stepResults.push(...results);

  // Combine all step outputs for finalOutput
  const finalOutput = stepResults
    .map(r => `[${r.step}]\n${r.output}`)
    .join('\n\n---\n\n');

  return {
    stepResults,
    finalOutput,
    totalDuration: Date.now() - startTime,
  };
}

/**
 * List all built-in flows.
 */
export function listBuiltInFlows(): Flow[] {
  return [...BUILT_IN_FLOWS];
}

/**
 * Get a built-in flow by name.
 */
export function getBuiltInFlow(name: string): Flow | undefined {
  return BUILT_IN_FLOWS.find(f => f.name === name);
}
