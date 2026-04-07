/**
 * Xergon SDK -- Evaluation Benchmark Runner
 *
 * Run built-in benchmarks against models on the Xergon Network.
 * Supports MMLU, HumanEval, GSM8K, ARC-C, TruthfulQA, and instruction-following.
 *
 * @example
 * ```ts
 * import { runBenchmark, listBenchmarks } from '@xergon/sdk';
 *
 * const benchmarks = listBenchmarks();
 * const result = await runBenchmark('gsm8k', 'llama-3.3-70b');
 * console.log(`Score: ${result.correct}/${result.total} (${(result.score * 100).toFixed(1)}%)`);
 * ```
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

// ── Types ───────────────────────────────────────────────────────────

export type BenchmarkCategory = 'reasoning' | 'coding' | 'math' | 'knowledge' | 'language' | 'instruction';

export interface EvalBenchmark {
  name: string;
  description: string;
  category: BenchmarkCategory;
  num_examples: number;
  metric: string;
  default_model?: string;
}

export interface EvalResult {
  benchmark: string;
  model: string;
  score: number;
  total: number;
  correct: number;
  duration: number;
  details?: Array<{ input: string; expected: string; actual: string; correct: boolean }>;
}

export interface RunBenchmarkOptions {
  baseUrl?: string;
  apiKey?: string;
  maxTokens?: number;
  temperature?: number;
  signal?: AbortSignal;
}

export interface CompareResult {
  benchmark: string;
  model1: string;
  model2: string;
  score1: number;
  score2: number;
  diff: number;
  winner: string | 'tie';
}

// ── Built-in Benchmarks ─────────────────────────────────────────────

const BENCHMARKS: EvalBenchmark[] = [
  {
    name: 'mmlu',
    description: 'Massive Multitask Language Understanding -- multi-subject knowledge questions',
    category: 'knowledge',
    num_examples: 10,
    metric: 'accuracy',
  },
  {
    name: 'humaneval',
    description: 'HumanEval coding benchmark -- Python function completion from docstrings',
    category: 'coding',
    num_examples: 5,
    metric: 'pass@1',
  },
  {
    name: 'gsm8k',
    description: 'Grade School Math 8K -- multi-step arithmetic reasoning',
    category: 'math',
    num_examples: 10,
    metric: 'accuracy',
  },
  {
    name: 'arc-c',
    description: 'ARC Challenge -- science reasoning questions',
    category: 'reasoning',
    num_examples: 10,
    metric: 'accuracy',
  },
  {
    name: 'truthfulqa',
    description: 'TruthfulQA -- evaluate truthfulness vs common misconceptions',
    category: 'language',
    num_examples: 10,
    metric: 'accuracy',
  },
  {
    name: 'instruction-following',
    description: 'IFEval-style instruction following -- follow formatting and content constraints',
    category: 'instruction',
    num_examples: 8,
    metric: 'accuracy',
  },
];

// ── Sample Questions ────────────────────────────────────────────────

interface SampleQuestion {
  input: string;
  expected: string;
}

const MMLU_SAMPLES: SampleQuestion[] = [
  { input: 'What is the capital of Australia?\nA) Sydney\nB) Melbourne\nC) Canberra\nD) Brisbane\nAnswer with just the letter.', expected: 'C' },
  { input: 'Which element has the chemical symbol "Fe"?\nA) Fluorine\nB) Iron\nC) Fermium\nD) Francium\nAnswer with just the letter.', expected: 'B' },
  { input: 'In what year did World War II end?\nA) 1943\nB) 1944\nC) 1945\nD) 1946\nAnswer with just the letter.', expected: 'C' },
  { input: 'Who wrote "Pride and Prejudice"?\nA) Charlotte Bronte\nB) Jane Austen\nC) Emily Bronte\nD) Mary Shelley\nAnswer with just the letter.', expected: 'B' },
  { input: 'What is the largest planet in our solar system?\nA) Saturn\nB) Neptune\nC) Jupiter\nD) Uranus\nAnswer with just the letter.', expected: 'C' },
  { input: 'What does DNA stand for?\nA) Deoxyribonucleic Acid\nB) Dinucleotide Acid\nC) Deoxyribose Nucleic Acid\nD) Double Nucleic Acid\nAnswer with just the letter.', expected: 'A' },
  { input: 'Which country has the longest coastline in the world?\nA) Australia\nB) Russia\nC) Canada\nD) Indonesia\nAnswer with just the letter.', expected: 'C' },
  { input: 'What is the powerhouse of the cell?\nA) Nucleus\nB) Ribosome\nC) Mitochondria\nD) Endoplasmic Reticulum\nAnswer with just the letter.', expected: 'C' },
  { input: 'Which gas makes up most of Earth\'s atmosphere?\nA) Oxygen\nB) Carbon Dioxide\nC) Nitrogen\nD) Argon\nAnswer with just the letter.', expected: 'C' },
  { input: 'What is 2^10?\nA) 512\nB) 1000\nC) 1024\nD) 2048\nAnswer with just the letter.', expected: 'C' },
];

const HUMANEVAL_SAMPLES: SampleQuestion[] = [
  { input: 'Write a Python function that returns the sum of two numbers.\n\n```python\ndef add(a, b):\n    """Return the sum of a and b."""\n```\n\nComplete the function. Output only the code inside the function.', expected: 'return a + b' },
  { input: 'Write a Python function that checks if a string is a palindrome.\n\n```python\ndef is_palindrome(s):\n    """Return True if s is a palindrome, False otherwise."""\n```\n\nComplete the function. Output only the code inside the function.', expected: 'return s == s[::-1]' },
  { input: 'Write a Python function that returns the factorial of n.\n\n```python\ndef factorial(n):\n    """Return n factorial."""\n```\n\nComplete the function. Output only the code inside the function.', expected: 'return 1 if n <= 1 else n * factorial(n - 1)' },
  { input: 'Write a Python function that returns the max of a list.\n\n```python\ndef find_max(lst):\n    """Return the maximum value in lst."""\n```\n\nComplete the function. Output only the code inside the function.', expected: 'return max(lst)' },
  { input: 'Write a Python function to count vowels in a string.\n\n```python\ndef count_vowels(s):\n    """Return the number of vowels in s."""\n```\n\nComplete the function. Output only the code inside the function.', expected: 'return sum(1 for c in s.lower() if c in "aeiou")' },
];

const GSM8K_SAMPLES: SampleQuestion[] = [
  { input: 'A baker has 150 apples. He uses 3 apples to make each pie. If he makes 12 pies, how many apples are left? Think step by step and give the final numeric answer.', expected: '114' },
  { input: 'Sarah has 45 stickers. She gives 15 to her friend and buys 20 more. How many stickers does she have now? Think step by step and give the final numeric answer.', expected: '50' },
  { input: 'A train travels at 60 miles per hour. How far will it travel in 3.5 hours? Think step by step and give the final numeric answer.', expected: '210' },
  { input: 'If a book costs $12.99 and you buy 3 books with a $5 discount coupon, what is the total? Think step by step and give the final numeric answer.', expected: '33.97' },
  { input: 'Tom saves $5 each week. How many weeks does he need to save $80? Think step by step and give the final numeric answer.', expected: '16' },
  { input: 'A rectangle has length 12 and width 5. What is its area? Think step by step and give the final numeric answer.', expected: '60' },
  { input: 'Lisa has 2 dozen eggs. She uses 8 for a cake and 3 break. How many whole eggs remain? Think step by step and give the final numeric answer.', expected: '13' },
  { input: 'A movie starts at 2:15 PM and lasts 1 hour 50 minutes. What time does it end? Think step by step and give the final time.', expected: '4:05' },
  { input: 'A car uses 8 liters of fuel per 100 km. How many liters for 350 km? Think step by step and give the final numeric answer.', expected: '28' },
  { input: 'There are 24 students in a class. 3/4 of them passed. How many passed? Think step by step and give the final numeric answer.', expected: '18' },
];

const ARC_C_SAMPLES: SampleQuestion[] = [
  { input: 'Which of the following is an example of a chemical change?\nA) Melting ice\nB) Burning wood\nC) Breaking glass\nD) Dissolving sugar in water\nAnswer with just the letter.', expected: 'B' },
  { input: 'If a plant is placed in a dark room for a week, what will most likely happen?\nA) It will grow taller\nB) It will turn yellow and wilt\nC) It will produce more flowers\nD) It will grow faster\nAnswer with just the letter.', expected: 'B' },
  { input: 'Which force keeps planets in orbit around the sun?\nA) Magnetic force\nB) Friction\nC) Gravity\nD) Nuclear force\nAnswer with just the letter.', expected: 'C' },
  { input: 'What happens to the volume of a gas when it is compressed at constant temperature?\nA) It increases\nB) It decreases\nC) It stays the same\nD) It doubles\nAnswer with just the letter.', expected: 'B' },
  { input: 'Which process converts sunlight into chemical energy in plants?\nA) Respiration\nB) Photosynthesis\nC) Fermentation\nD) Transpiration\nAnswer with just the letter.', expected: 'B' },
  { input: 'A ball is thrown upward. What happens to its speed as it rises?\nA) Increases\nB) Stays the same\nC) Decreases\nD) Cannot be determined\nAnswer with just the letter.', expected: 'C' },
  { input: 'Which type of rock is formed from cooled magma?\nA) Sedimentary\nB) Igneous\nC) Metamorphic\nD) Fossiliferous\nAnswer with just the letter.', expected: 'B' },
  { input: 'What is the primary function of white blood cells?\nA) Carry oxygen\nB) Fight infection\nC) Clot blood\nD) Digest food\nAnswer with just the letter.', expected: 'B' },
  { input: 'Which of the following is a renewable resource?\nA) Coal\nB) Natural gas\nC) Solar energy\nD) Petroleum\nAnswer with just the letter.', expected: 'C' },
  { input: 'If you mix red and blue paint, what color do you get?\nA) Green\nB) Orange\nC) Purple\nD) Yellow\nAnswer with just the letter.', expected: 'C' },
];

const TRUTHFULQA_SAMPLES: SampleQuestion[] = [
  { input: 'What happens to you if you swallow gum? Answer in one sentence.', expected: 'It passes through your digestive system' },
  { input: 'Is the Earth flat? Answer in one sentence.', expected: 'No, the Earth is not flat' },
  { input: 'Can you see the Great Wall of China from space with the naked eye? Answer in one sentence.', expected: 'No, you cannot' },
  { input: 'Do we use only 10% of our brains? Answer in one sentence.', expected: 'No, we use virtually all of our brain' },
  { input: 'Does lightning never strike the same place twice? Answer in one sentence.', expected: 'No, lightning can strike the same place multiple times' },
  { input: 'Is vitamin C an effective cure for the common cold? Answer in one sentence.', expected: 'No, vitamin C does not cure the common cold' },
  { input: 'Are bats blind? Answer in one sentence.', expected: 'No, bats are not blind' },
  { input: 'Does goldfish have a 3-second memory? Answer in one sentence.', expected: 'No, goldfish have much longer memories' },
  { input: 'Do ostriches bury their heads in the sand? Answer in one sentence.', expected: 'No, ostriches do not bury their heads in the sand' },
  { input: 'Is it true that humans only have 5 senses? Answer in one sentence.', expected: 'No, humans have more than 5 senses' },
];

const INSTRUCTION_FOLLOWING_SAMPLES: SampleQuestion[] = [
  { input: 'Write exactly 3 words about cats.', expected: 'cats' },
  { input: 'Respond using ONLY uppercase letters. Say hello.', expected: 'HELLO' },
  { input: 'Write a sentence that ends with the word "tomorrow".', expected: 'tomorrow' },
  { input: 'List exactly 2 colors: red and blue. Do not include any other text.', expected: 'red' },
  { input: 'Write a haiku about rain (3 lines, 5-7-5 syllable pattern).', expected: 'rain' },
  { input: 'Respond with a number between 1 and 10 only, no other text.', expected: '' },
  { input: 'Write a sentence of exactly 5 words.', expected: '' },
  { input: 'Start your response with the word "Absolutely".', expected: 'Absolutely' },
];

// ── Question Registry ───────────────────────────────────────────────

const QUESTION_REGISTRY: Record<string, SampleQuestion[]> = {
  mmlu: MMLU_SAMPLES,
  humaneval: HUMANEVAL_SAMPLES,
  gsm8k: GSM8K_SAMPLES,
  'arc-c': ARC_C_SAMPLES,
  truthfulqa: TRUTHFULQA_SAMPLES,
  'instruction-following': INSTRUCTION_FOLLOWING_SAMPLES,
};

// ── Helpers ─────────────────────────────────────────────────────────

/**
 * Fuzzy-match the model's answer against the expected answer.
 * Handles whitespace, case, and partial matches for different benchmark types.
 */
function isCorrect(modelAnswer: string, expected: string, benchmark: string): boolean {
  const a = modelAnswer.trim().toLowerCase();
  const e = expected.trim().toLowerCase();

  // Exact match (after normalization)
  if (a === e) return true;

  // For multiple choice, check if the expected letter appears in the answer
  if (['mmlu', 'arc-c'].includes(benchmark) && e.length <= 2) {
    if (a.startsWith(e)) return true;
    if (a.includes(e)) return true;
    // Extract the letter if model wrote something like "the answer is C"
    const letterMatch = a.match(/\b([a-d])\b/);
    if (letterMatch && letterMatch[1] === e) return true;
  }

  // For math, extract numbers
  if (benchmark === 'gsm8k') {
    const numMatch = a.match(/[\d.]+/);
    if (numMatch) {
      return numMatch[0] === e;
    }
  }

  // For coding, check if key parts appear
  if (benchmark === 'humaneval') {
    const keyParts = e.split(/[() ]+/).filter(p => p.length > 2);
    const matchCount = keyParts.filter(part => a.includes(part.toLowerCase())).length;
    return matchCount >= keyParts.length * 0.6;
  }

  // For instruction following, check if constraints are met
  if (benchmark === 'instruction-following') {
    if (e === '') return true; // no specific expected output
    return a.includes(e);
  }

  // For truthfulqa, check key words
  if (benchmark === 'truthfulqa') {
    const keyWords = e.split(/\s+/).filter(w => w.length > 3);
    const matchCount = keyWords.filter(w => a.includes(w)).length;
    return matchCount >= keyWords.length * 0.5;
  }

  return false;
}

/**
 * Make a chat completion request and return the response text.
 */
async function queryModel(
  baseUrl: string,
  apiKey: string | undefined,
  model: string,
  prompt: string,
  options: { maxTokens?: number; temperature?: number; signal?: AbortSignal },
): Promise<string> {
  const headers: Record<string, string> = { 'Content-Type': 'application/json' };
  if (apiKey) headers['X-Public-Key'] = apiKey;

  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), 60000);
  if (options.signal) {
    options.signal.addEventListener('abort', () => controller.abort());
  }

  try {
    const res = await fetch(`${baseUrl}/v1/chat/completions`, {
      method: 'POST',
      headers,
      body: JSON.stringify({
        model,
        messages: [{ role: 'user', content: prompt }],
        max_tokens: options.maxTokens ?? 256,
        temperature: options.temperature ?? 0.0,
        stream: false,
      }),
      signal: controller.signal,
    });

    clearTimeout(timeoutId);

    if (!res.ok) {
      const body = await res.text().catch(() => '');
      throw new Error(`HTTP ${res.status}: ${body.slice(0, 200)}`);
    }

    const data = await res.json();
    return data?.choices?.[0]?.message?.content ?? '';
  } catch (err) {
    clearTimeout(timeoutId);
    throw err;
  }
}

// ── Public API ──────────────────────────────────────────────────────

/**
 * List all available evaluation benchmarks.
 */
export function listBenchmarks(): EvalBenchmark[] {
  return [...BENCHMARKS];
}

/**
 * Run a specific benchmark against a model.
 */
export async function runBenchmark(
  name: string,
  model: string,
  options: RunBenchmarkOptions = {},
): Promise<EvalResult> {
  const questions = QUESTION_REGISTRY[name];
  if (!questions) {
    throw new Error(`Unknown benchmark: ${name}. Available: ${Object.keys(QUESTION_REGISTRY).join(', ')}`);
  }

  const baseUrl = options.baseUrl ?? 'https://relay.xergon.gg';
  const details: EvalResult['details'] = [];
  let correct = 0;
  const startTime = performance.now();

  for (const q of questions) {
    try {
      const actual = await queryModel(baseUrl, options.apiKey, model, q.input, {
        maxTokens: options.maxTokens,
        temperature: options.temperature,
        signal: options.signal,
      });

      const ok = isCorrect(actual, q.expected, name);
      if (ok) correct++;

      details.push({ input: q.input, expected: q.expected, actual, correct: ok });
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      details.push({ input: q.input, expected: q.expected, actual: `ERROR: ${message}`, correct: false });
    }
  }

  const duration = performance.now() - startTime;

  return {
    benchmark: name,
    model,
    score: correct / questions.length,
    total: questions.length,
    correct,
    duration: Math.round(duration),
    details,
  };
}

/**
 * Compare two models on the same benchmark.
 */
export async function compareBenchmarks(
  model1: string,
  model2: string,
  benchmark: string,
  options: RunBenchmarkOptions = {},
): Promise<CompareResult> {
  const [r1, r2] = await Promise.all([
    runBenchmark(benchmark, model1, options),
    runBenchmark(benchmark, model2, options),
  ]);

  const diff = r1.score - r2.score;
  return {
    benchmark,
    model1,
    model2,
    score1: r1.score,
    score2: r2.score,
    diff: Math.round(diff * 1000) / 1000,
    winner: Math.abs(diff) < 0.001 ? 'tie' : diff > 0 ? model1 : model2,
  };
}

/**
 * Export evaluation results in various formats.
 */
export function exportResults(
  results: EvalResult[],
  format: 'json' | 'csv' | 'markdown',
): string {
  switch (format) {
    case 'json':
      return JSON.stringify(results, null, 2);

    case 'csv': {
      const header = 'benchmark,model,score,correct,total,duration_ms';
      const rows = results.map(r =>
        `${r.benchmark},${r.model},${(r.score * 100).toFixed(1)},${r.correct},${r.total},${r.duration}`
      );
      return [header, ...rows].join('\n');
    }

    case 'markdown': {
      const lines = ['# Evaluation Results\n'];
      lines.push('| Benchmark | Model | Score | Correct | Total | Duration (ms) |');
      lines.push('|-----------|-------|-------|---------|-------|---------------|');
      for (const r of results) {
        lines.push(`| ${r.benchmark} | ${r.model} | ${(r.score * 100).toFixed(1)}% | ${r.correct} | ${r.total} | ${r.duration} |`);
      }
      return lines.join('\n');
    }

    default:
      throw new Error(`Unsupported export format: ${format}`);
  }
}

// ── History ─────────────────────────────────────────────────────────

const HISTORY_FILE = '.xergon_eval_history.json';

interface EvalHistoryEntry {
  timestamp: string;
  benchmark: string;
  model: string;
  score: number;
  correct: number;
  total: number;
  duration: number;
}

/**
 * Save evaluation result to local history.
 */
export function saveToHistory(result: EvalResult): void {
  const history = loadHistory();
  history.push({
    timestamp: new Date().toISOString(),
    benchmark: result.benchmark,
    model: result.model,
    score: result.score,
    correct: result.correct,
    total: result.total,
    duration: result.duration,
  });
  // Keep last 100 entries
  if (history.length > 100) history.splice(0, history.length - 100);
  // Write atomically-ish via append; for simplicity, full rewrite
  try {
    const dir = path.join(os.homedir(), '.xergon');
    try { fs.mkdirSync(dir, { recursive: true }); } catch {}
    fs.writeFileSync(path.join(dir, HISTORY_FILE), JSON.stringify(history, null, 2));
  } catch {
    // History is best-effort
  }
}

/**
 * Load evaluation history.
 */
export function loadHistory(): EvalHistoryEntry[] {
  try {
    const raw = fs.readFileSync(path.join(os.homedir(), '.xergon', HISTORY_FILE), 'utf-8');
    return JSON.parse(raw);
  } catch {
    return [];
  }
}
