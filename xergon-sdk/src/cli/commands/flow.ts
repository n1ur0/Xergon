/**
 * CLI command: flow
 *
 * List, show, run, and create LLM flows/pipelines.
 */

import type { Command, CommandOption, ParsedArgs, CLIContext } from '../mod';
import {
  listBuiltInFlows,
  getBuiltInFlow,
  createFlow,
  runFlow,
} from '../../flow';
import type { Flow, FlowExecutor } from '../../flow';

const flowListOptions: CommandOption[] = [];
const flowShowOptions: CommandOption[] = [];
const flowRunOptions: CommandOption[] = [
  {
    name: 'input',
    short: '',
    long: '--input',
    description: 'Input text to process through the flow',
    required: true,
    type: 'string',
  },
  {
    name: 'parallel',
    short: '',
    long: '--parallel',
    description: 'Run independent steps in parallel',
    required: false,
    type: 'boolean',
  },
];
const flowCreateOptions: CommandOption[] = [
  {
    name: 'name',
    short: '',
    long: '--name',
    description: 'Name for the custom flow',
    required: true,
    type: 'string',
  },
  {
    name: 'steps',
    short: '',
    long: '--steps',
    description: 'Pipe-separated step names: "step1|step2|step3"',
    required: true,
    type: 'string',
  },
  {
    name: 'description',
    short: '',
    long: '--description',
    description: 'Description for the custom flow',
    required: false,
    type: 'string',
  },
];

async function flowAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const subCommand = args.positional[0];

  switch (subCommand) {
    case 'list':
      await handleList(ctx);
      break;
    case 'show':
      await handleShow(args, ctx);
      break;
    case 'run':
      await handleRun(args, ctx);
      break;
    case 'create':
      await handleCreate(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown flow subcommand: ${subCommand || '(none)'}`);
      ctx.output.info('Usage: xergon flow <list|show|run|create> [options]');
      process.exit(1);
  }
}

async function handleList(ctx: CLIContext): Promise<void> {
  const flows = listBuiltInFlows();

  ctx.output.write(ctx.output.colorize('Available Flows:\n', 'bold'));
  ctx.output.write(ctx.output.colorize('─'.repeat(60) + '\n', 'dim'));

  for (const flow of flows) {
    ctx.output.write(`  ${ctx.output.colorize(flow.name, 'cyan')}  ${flow.description}\n`);
    ctx.output.write(`    Steps: ${flow.steps.map(s => s.name).join(' -> ')}\n\n`);
  }

  ctx.output.write(ctx.output.colorize(`  ${flows.length} built-in flow(s)\n`, 'dim'));
}

async function handleShow(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.positional[1];
  if (!name) {
    ctx.output.writeError('Usage: xergon flow show <name>');
    process.exit(1);
  }

  const flow = getBuiltInFlow(name);
  if (!flow) {
    ctx.output.writeError(`Flow not found: ${name}`);
    ctx.output.info('Run "xergon flow list" to see available flows.');
    process.exit(1);
    return; // unreachable
  }

  ctx.output.write(ctx.output.colorize(`Flow: ${flow.name}\n`, 'bold'));
  ctx.output.write(`  ${flow.description}\n\n`);
  ctx.output.write(ctx.output.colorize('Steps:\n', 'bold'));
  ctx.output.write(ctx.output.colorize('─'.repeat(40) + '\n', 'dim'));

  for (let i = 0; i < flow.steps.length; i++) {
    const step = flow.steps[i];
    const arrow = i < flow.steps.length - 1 ? ' -> ' : '';
    ctx.output.write(`  ${ctx.output.colorize(`${i + 1}.`, 'yellow')} ${ctx.output.colorize(step.name, 'cyan')}${arrow}\n`);
    if (step.systemPrompt) {
      const preview = step.systemPrompt.length > 80
        ? step.systemPrompt.substring(0, 80) + '...'
        : step.systemPrompt;
      ctx.output.write(`     ${ctx.output.colorize(preview, 'dim')}\n`);
    }
    if (step.model) {
      ctx.output.write(`     Model: ${ctx.output.colorize(step.model, 'green')}\n`);
    }
    if (step.transform) {
      ctx.output.write(`     ${ctx.output.colorize('(has transform)', 'dim')}\n`);
    }
  }
  ctx.output.write('\n');
}

async function handleRun(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.positional[1];
  const input = args.options.input ? String(args.options.input) : undefined;
  const parallel = args.options.parallel === true;

  if (!name) {
    ctx.output.writeError('Usage: xergon flow run <name> --input "text"');
    process.exit(1);
  }
  if (!input) {
    ctx.output.writeError('Missing required option: --input');
    process.exit(1);
  }

  let flow = getBuiltInFlow(name);
  if (!flow) {
    ctx.output.writeError(`Flow not found: ${name}`);
    ctx.output.info('Run "xergon flow list" to see available flows.');
    process.exit(1);
    return; // unreachable
  }

  ctx.output.info(`Running flow: ${flow.name}`);
  if (parallel) {
    ctx.output.info('Mode: parallel');
  }

  // Build executor from CLI client
  const model = String(ctx.config.defaultModel || 'llama-3.3-70b');
  const executor: FlowExecutor = async (stepModel, messages) => {
    const response = await ctx.client.chat.completions.create({
      model: stepModel || model,
      messages,
    });
    return { content: response.choices?.[0]?.message?.content || '(no content)' };
  };

  try {
    const result = parallel
      ? await (await import('../../flow')).runFlowParallel(flow!, input!, executor, model)
      : await runFlow(flow!, input!, executor, model);

    ctx.output.write(ctx.output.colorize('\nResults:\n', 'bold'));
    ctx.output.write(ctx.output.colorize('─'.repeat(40) + '\n', 'dim'));

    for (const sr of result.stepResults) {
      ctx.output.write(`  ${ctx.output.colorize(sr.step, 'cyan')} (${sr.duration}ms)\n`);
      const preview = sr.output.length > 200
        ? sr.output.substring(0, 200) + '...'
        : sr.output;
      ctx.output.write(`  ${preview}\n\n`);
    }

    ctx.output.write(ctx.output.colorize('─'.repeat(40) + '\n', 'dim'));
    ctx.output.write(`  ${ctx.output.colorize(`Total: ${result.totalDuration}ms`, 'yellow')}\n`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Flow execution failed: ${message}`);
    process.exit(1);
  }
}

async function handleCreate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const name = args.options.name ? String(args.options.name) : undefined;
  const stepsStr = args.options.steps ? String(args.options.steps) : undefined;
  const description = args.options.description
    ? String(args.options.description)
    : `Custom flow: ${name || 'unnamed'}`;

  if (!name) {
    ctx.output.writeError('Missing required option: --name');
    process.exit(1);
    return; // unreachable
  }
  if (!stepsStr) {
    ctx.output.writeError('Missing required option: --steps');
    process.exit(1);
    return; // unreachable
  }

  const stepNames = stepsStr.split('|').map(s => s.trim()).filter(Boolean);
  if (stepNames.length === 0) {
    ctx.output.writeError('No steps provided. Use pipe-separated names: "step1|step2|step3"');
    process.exit(1);
  }

  const steps = stepNames.map(s => ({
    name: s,
    systemPrompt: `Process the input for step: ${s}`,
  }));

  const flow = createFlow(name, description, steps);

  ctx.output.success(`Flow created: ${flow.name}`);
  ctx.output.write(`  Description: ${flow.description}\n`);
  ctx.output.write(`  Steps: ${flow.steps.map(s => s.name).join(' -> ')}\n`);
}

export const flowCommand: Command = {
  name: 'flow',
  description: 'List, show, run, and create LLM flows/pipelines',
  aliases: ['pipeline'],
  options: [
    ...flowListOptions,
    ...flowShowOptions,
    ...flowRunOptions,
    ...flowCreateOptions,
  ],
  action: flowAction,
};
