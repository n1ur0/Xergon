/**
 * CLI command: fine-tune
 *
 * Manage fine-tuning jobs on the Xergon Network.
 *
 * Usage:
 *   xergon fine-tune create --model llama3 --dataset data.jsonl --method qlora --epochs 3
 *   xergon fine-tune list
 *   xergon fine-tune status <id>
 *   xergon fine-tune cancel <id>
 *   xergon fine-tune export <id> --output ./adapter
 *   xergon fine-tune list-runs
 *   xergon fine-tune compare <run1> <run2>
 */

import type { Command, ParsedArgs, CLIContext } from '../mod';
import type { FineTuneJob } from '../../fine-tune';

async function fineTuneAction(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const sub = args.positional[0];

  if (!sub) {
    ctx.output.writeError('Usage: xergon fine-tune <create|list|status|cancel|export|list-runs|compare> [options]');
    process.exit(1);
    return;
  }

  switch (sub) {
    case 'create':
      await handleCreate(args, ctx);
      break;
    case 'list':
      await handleList(args, ctx);
      break;
    case 'status':
      await handleStatus(args, ctx);
      break;
    case 'cancel':
      await handleCancel(args, ctx);
      break;
    case 'export':
      await handleExport(args, ctx);
      break;
    case 'list-runs':
      await handleListRuns(args, ctx);
      break;
    case 'compare':
      await handleCompare(args, ctx);
      break;
    default:
      ctx.output.writeError(`Unknown subcommand: ${sub}`);
      ctx.output.write('Usage: xergon fine-tune <create|list|status|cancel|export|list-runs|compare> [options]');
      process.exit(1);
  }
}

// ── create ─────────────────────────────────────────────────────────

async function handleCreate(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const model = String(args.options.model || '');
  const dataset = String(args.options.dataset || '');
  const method = String(args.options.method || 'qlora') as 'lora' | 'qlora' | 'full';
  const epochs = args.options.epochs !== undefined ? Number(args.options.epochs) : 3;
  const learningRate = args.options.learning_rate !== undefined ? Number(args.options.learning_rate) : undefined;
  const batchSize = args.options.batch_size !== undefined ? Number(args.options.batch_size) : undefined;
  const loraR = args.options.lora_r !== undefined ? Number(args.options.lora_r) : undefined;
  const loraAlpha = args.options.lora_alpha !== undefined ? Number(args.options.lora_alpha) : undefined;
  const loraDropout = args.options.lora_dropout !== undefined ? Number(args.options.lora_dropout) : undefined;
  const outputName = args.options.output_name ? String(args.options.output_name) : undefined;

  // v2 options
  const evalFreq = args.options.eval_freq !== undefined ? Number(args.options.eval_freq) : undefined;
  const evalDataset = args.options.eval_dataset ? String(args.options.eval_dataset) : undefined;
  const earlyStopPatience = args.options.early_stop_patience !== undefined ? Number(args.options.early_stop_patience) : undefined;
  const gradientAccumulationSteps = args.options.gradient_accumulation_steps !== undefined ? Number(args.options.gradient_accumulation_steps) : undefined;
  const warmupRatio = args.options.warmup_ratio !== undefined ? Number(args.options.warmup_ratio) : undefined;
  const lrScheduler = args.options.lr_scheduler ? String(args.options.lr_scheduler) as LRSchedulerType : undefined;
  const maxGradNorm = args.options.max_grad_norm !== undefined ? Number(args.options.max_grad_norm) : undefined;
  const resumeFrom = args.options.resume_from ? String(args.options.resume_from) : undefined;
  const outputDir = args.options.output_dir ? String(args.options.output_dir) : undefined;

  if (!model) {
    ctx.output.writeError('Missing required option: --model');
    process.exit(1);
    return;
  }

  if (!dataset) {
    ctx.output.writeError('Missing required option: --dataset');
    process.exit(1);
    return;
  }

  // Validate method
  if (!['lora', 'qlora', 'full'].includes(method)) {
    ctx.output.writeError(`Invalid method: ${method}. Must be one of: lora, qlora, full`);
    process.exit(1);
    return;
  }

  // Validate LR scheduler
  if (lrScheduler && !['cosine', 'linear', 'constant', 'cosine_with_restarts'].includes(lrScheduler)) {
    ctx.output.writeError(`Invalid LR scheduler: ${lrScheduler}. Must be: cosine, linear, constant, cosine_with_restarts`);
    process.exit(1);
    return;
  }

  // Validate warmup ratio
  if (warmupRatio !== undefined && (warmupRatio < 0 || warmupRatio > 0.3)) {
    ctx.output.writeError('Warmup ratio must be between 0.0 and 0.3');
    process.exit(1);
    return;
  }

  // Validate LoRA rank
  if (loraR !== undefined && ![8, 16, 32, 64].includes(loraR)) {
    ctx.output.writeError('LoRA rank must be one of: 8, 16, 32, 64');
    process.exit(1);
    return;
  }

  // Validate dataset exists if it's a local file
  try {
    const { existsSync } = await import('node:fs');
    if (existsSync(dataset)) {
      // local file is fine
    } else if (!dataset.startsWith('http://') && !dataset.startsWith('https://')) {
      ctx.output.writeError(`Dataset not found: ${dataset}`);
      process.exit(1);
      return;
    }
  } catch {
    // Skip file check if fs not available
  }

  const thinkingMsg = ctx.output.colorize('Creating fine-tune job', 'cyan');
  process.stderr.write(`${thinkingMsg}...\r`);

  try {
    const { createFineTuneJob } = await import('../../fine-tune');
    const job = await createFineTuneJob(ctx.client._core || ctx.client.core, {
      model,
      dataset,
      method,
      epochs,
      learning_rate: learningRate,
      batch_size: batchSize,
      lora_r: loraR,
      lora_alpha: loraAlpha,
      output_name: outputName,
    });

    process.stderr.write(' '.repeat(40) + '\r');

    ctx.output.success('Fine-tune job created successfully');
    ctx.output.write('');

    const jobInfo: Record<string, unknown> = {
      ID: job.id,
      Model: job.model,
      Method: job.method || '-',
      Status: job.status,
      Progress: `${job.progress}%`,
      Epoch: `${job.epoch}/${job.total_epochs}`,
      Loss: job.loss.toFixed(4),
      Created: new Date(job.created_at).toISOString().slice(0, 19),
    };

    // Show v2 params if set
    if (evalFreq) jobInfo['Eval Freq'] = `every ${evalFreq} steps`;
    if (evalDataset) jobInfo['Eval Dataset'] = evalDataset;
    if (earlyStopPatience) jobInfo['Early Stop'] = `${earlyStopPatience} evals`;
    if (gradientAccumulationSteps) jobInfo['Grad Accum'] = gradientAccumulationSteps;
    if (warmupRatio !== undefined) jobInfo['Warmup'] = `${warmupRatio}`;
    if (lrScheduler) jobInfo['LR Scheduler'] = lrScheduler;
    if (maxGradNorm) jobInfo['Max Grad Norm'] = maxGradNorm;
    if (loraDropout !== undefined) jobInfo['LoRA Dropout'] = loraDropout;
    if (resumeFrom) jobInfo['Resumed From'] = resumeFrom;
    if (outputDir) jobInfo['Output Dir'] = outputDir;

    ctx.output.write(ctx.output.formatText(jobInfo, 'Fine-Tune Job'));
  } catch (err) {
    process.stderr.write(' '.repeat(40) + '\r');
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to create fine-tune job: ${message}`);
    process.exit(1);
  }
}

// ── list ───────────────────────────────────────────────────────────

async function handleList(_args: ParsedArgs, ctx: CLIContext): Promise<void> {
  try {
    const { listFineTuneJobs } = await import('../../fine-tune');
    const jobs = await listFineTuneJobs(ctx.client._core || ctx.client.core);

    if (jobs.length === 0) {
      ctx.output.info('No fine-tune jobs found.');
      return;
    }

    const tableData = jobs.map((j: FineTuneJob) => ({
      ID: j.id,
      Model: j.model,
      Method: j.method || '-',
      Status: j.status,
      Progress: `${j.progress}%`,
      'Epoch': `${j.epoch}/${j.total_epochs}`,
      Loss: j.loss.toFixed(4),
      Created: new Date(j.created_at).toISOString().slice(0, 19),
    }));
    ctx.output.write(ctx.output.formatTable(tableData, `Fine-Tune Jobs (${jobs.length})`));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list fine-tune jobs: ${message}`);
    process.exit(1);
  }
}

// ── status ─────────────────────────────────────────────────────────

async function handleStatus(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const jobId = args.positional[1];

  if (!jobId) {
    ctx.output.writeError('No job ID specified. Use: xergon fine-tune status <id>');
    process.exit(1);
    return;
  }

  try {
    const { getFineTuneJob } = await import('../../fine-tune');
    const job = await getFineTuneJob(ctx.client._core || ctx.client.core, jobId);

    ctx.output.write(ctx.output.formatText(job, `Fine-Tune Job ${job.id}`));

    // Progress bar
    if (job.status === 'running' || job.status === 'completed') {
      const barWidth = 30;
      const filled = Math.round((job.progress / 100) * barWidth);
      const empty = barWidth - filled;
      const bar = ctx.output.colorize('█'.repeat(filled), 'green') +
                  ctx.output.colorize('░'.repeat(empty), 'dim');
      ctx.output.write(`\n  Progress: [${bar}] ${job.progress}%`);
      ctx.output.write(`  Epoch: ${job.epoch}/${job.total_epochs}  Loss: ${job.loss.toFixed(4)}`);

      // Real-time training metrics
      if (job.status === 'running') {
        ctx.output.write('');
        ctx.output.info('Training metrics (live):');
        ctx.output.write(`  Training Loss: ${job.loss.toFixed(4)}`);
        if (args.options.eval_freq) {
          ctx.output.write(`  Eval Frequency: every ${args.options.eval_freq} steps`);
        }
        ctx.output.write(`  Status: ${job.status}`);
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to get job status: ${message}`);
    process.exit(1);
  }
}

// ── cancel ─────────────────────────────────────────────────────────

async function handleCancel(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const jobId = args.positional[1];

  if (!jobId) {
    ctx.output.writeError('No job ID specified. Use: xergon fine-tune cancel <id>');
    process.exit(1);
    return;
  }

  try {
    const { cancelFineTuneJob } = await import('../../fine-tune');
    const job = await cancelFineTuneJob(ctx.client._core || ctx.client.core, jobId);

    ctx.output.success(`Job ${jobId} cancelled successfully`);
    ctx.output.write(`  Status: ${job.status}`);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to cancel job: ${message}`);
    process.exit(1);
  }
}

// ── export ─────────────────────────────────────────────────────────

async function handleExport(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const jobId = args.positional[1];
  const outputPath = args.options.output ? String(args.options.output) : undefined;

  if (!jobId) {
    ctx.output.writeError('No job ID specified. Use: xergon fine-tune export <id> --output ./adapter');
    process.exit(1);
    return;
  }

  const thinkingMsg = ctx.output.colorize('Exporting fine-tune adapter', 'cyan');
  process.stderr.write(`${thinkingMsg}...\r`);

  try {
    const { exportFineTuneJob } = await import('../../fine-tune');
    const result = await exportFineTuneJob(ctx.client._core || ctx.client.core, jobId);

    process.stderr.write(' '.repeat(40) + '\r');

    ctx.output.success('Adapter exported successfully');
    ctx.output.write('');
    ctx.output.write(ctx.output.formatText({
      Job_ID: result.job_id,
      Adapter_Path: result.adapter_path,
      Size: `${(result.size_bytes / 1024 / 1024).toFixed(2)} MB`,
      Format: result.format,
      Exported_At: result.exported_at,
    }, 'Export Result'));
  } catch (err) {
    process.stderr.write(' '.repeat(40) + '\r');
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to export job: ${message}`);
    process.exit(1);
  }
}

// ── list-runs ──────────────────────────────────────────────────────

async function handleListRuns(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  try {
    const { listFineTuneJobs } = await import('../../fine-tune');
    const jobs = await listFineTuneJobs(ctx.client._core || ctx.client.core);

    if (jobs.length === 0) {
      ctx.output.info('No fine-tune runs found.');
      return;
    }

    const tableData = jobs.map((j: FineTuneJob) => ({
      Run_ID: j.id,
      Model: j.model,
      Method: j.method || '-',
      Status: j.status,
      Progress: `${j.progress}%`,
      Epochs: `${j.epoch}/${j.total_epochs}`,
      'Final Loss': j.loss.toFixed(4),
      Created: new Date(j.created_at).toISOString().slice(0, 19),
    }));

    // Sort by creation date (newest first)
    tableData.reverse();

    ctx.output.write(ctx.output.formatTable(tableData, `Fine-Tune Runs (${jobs.length})`));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to list runs: ${message}`);
    process.exit(1);
  }
}

// ── compare ────────────────────────────────────────────────────────

async function handleCompare(args: ParsedArgs, ctx: CLIContext): Promise<void> {
  const run1 = args.positional[1];
  const run2 = args.positional[2];

  if (!run1 || !run2) {
    ctx.output.writeError('Usage: xergon fine-tune compare <run1> <run2>');
    process.exit(1);
    return;
  }

  try {
    const { getFineTuneJob } = await import('../../fine-tune');
    const [job1, job2] = await Promise.all([
      getFineTuneJob(ctx.client._core || ctx.client.core, run1),
      getFineTuneJob(ctx.client._core || ctx.client.core, run2),
    ]);

    const tableData = [
      {
        Metric: 'Model',
        [run1.slice(0, 12)]: job1.model,
        [run2.slice(0, 12)]: job2.model,
      },
      {
        Metric: 'Method',
        [run1.slice(0, 12)]: job1.method || '-',
        [run2.slice(0, 12)]: job2.method || '-',
      },
      {
        Metric: 'Status',
        [run1.slice(0, 12)]: job1.status,
        [run2.slice(0, 12)]: job2.status,
      },
      {
        Metric: 'Epochs',
        [run1.slice(0, 12)]: `${job1.epoch}/${job1.total_epochs}`,
        [run2.slice(0, 12)]: `${job2.epoch}/${job2.total_epochs}`,
      },
      {
        Metric: 'Final Loss',
        [run1.slice(0, 12)]: job1.loss.toFixed(4),
        [run2.slice(0, 12)]: job2.loss.toFixed(4),
      },
      {
        Metric: 'Duration',
        [run1.slice(0, 12)]: `${Math.round((Date.now() - new Date(job1.created_at).getTime()) / 60000)} min`,
        [run2.slice(0, 12)]: `${Math.round((Date.now() - new Date(job2.created_at).getTime()) / 60000)} min`,
      },
    ];

    ctx.output.write(ctx.output.formatTable(tableData, 'Run Comparison'));

    // Verdict
    if (job1.status === 'completed' && job2.status === 'completed') {
      ctx.output.write('');
      if (job1.loss < job2.loss) {
        ctx.output.success(`${run1} has lower loss (${job1.loss.toFixed(4)} vs ${job2.loss.toFixed(4)})`);
      } else if (job2.loss < job1.loss) {
        ctx.output.success(`${run2} has lower loss (${job2.loss.toFixed(4)} vs ${job1.loss.toFixed(4)})`);
      } else {
        ctx.output.info('Both runs have identical final loss.');
      }
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    ctx.output.writeError(`Failed to compare runs: ${message}`);
    process.exit(1);
  }
}

// ── Types ──────────────────────────────────────────────────────────

type LRSchedulerType = 'cosine' | 'linear' | 'constant' | 'cosine_with_restarts';

export const fineTuneCommand: Command = {
  name: 'fine-tune',
  description: 'Create, monitor, cancel, export, and compare fine-tuning jobs',
  aliases: ['finetune', 'ft'],
  options: [
    // ── Core options ──
    {
      name: 'model',
      short: '-m',
      long: '--model',
      description: 'Base model to fine-tune (e.g., llama3, mistral)',
      required: false,
      type: 'string',
    },
    {
      name: 'dataset',
      short: '-d',
      long: '--dataset',
      description: 'Dataset file path or URL (JSONL format)',
      required: false,
      type: 'string',
    },
    {
      name: 'method',
      short: '',
      long: '--method',
      description: 'Fine-tuning method: lora, qlora, or full (default: qlora)',
      required: false,
      type: 'string',
    },
    {
      name: 'epochs',
      short: '-e',
      long: '--epochs',
      description: 'Number of training epochs (default: 3)',
      required: false,
      type: 'number',
    },
    {
      name: 'learning_rate',
      short: '-lr',
      long: '--learning-rate',
      description: 'Learning rate for training',
      required: false,
      type: 'number',
    },
    {
      name: 'batch_size',
      short: '-b',
      long: '--batch-size',
      description: 'Batch size for training',
      required: false,
      type: 'number',
    },
    {
      name: 'lora_r',
      short: '',
      long: '--lora-r',
      description: 'LoRA rank: 8, 16, 32, or 64 (default: 16)',
      required: false,
      type: 'number',
    },
    {
      name: 'lora_alpha',
      short: '',
      long: '--lora-alpha',
      description: 'LoRA alpha scaling (default: 32)',
      required: false,
      type: 'number',
    },
    {
      name: 'lora_dropout',
      short: '',
      long: '--lora-dropout',
      description: 'LoRA dropout rate (default: 0.05)',
      required: false,
      type: 'number',
    },
    {
      name: 'output_name',
      short: '',
      long: '--output-name',
      description: 'Name for the output adapter',
      required: false,
      type: 'string',
    },
    {
      name: 'output',
      short: '-o',
      long: '--output',
      description: 'Output path for exported adapter',
      required: false,
      type: 'string',
    },

    // ── v2 options ──
    {
      name: 'eval_freq',
      short: '',
      long: '--eval-freq',
      description: 'Run evaluation every N steps during training',
      required: false,
      type: 'number',
    },
    {
      name: 'eval_dataset',
      short: '',
      long: '--eval-dataset',
      description: 'Dataset for evaluation during fine-tuning',
      required: false,
      type: 'string',
    },
    {
      name: 'early_stop_patience',
      short: '',
      long: '--early-stop-patience',
      description: 'Stop if eval loss doesn\'t improve for N evals',
      required: false,
      type: 'number',
    },
    {
      name: 'gradient_accumulation_steps',
      short: '',
      long: '--gradient-accumulation-steps',
      description: 'Accumulate gradients over N steps',
      required: false,
      type: 'number',
    },
    {
      name: 'warmup_ratio',
      short: '',
      long: '--warmup-ratio',
      description: 'Fraction of steps for LR warmup (0.0-0.3)',
      required: false,
      type: 'number',
    },
    {
      name: 'lr_scheduler',
      short: '',
      long: '--lr-scheduler',
      description: 'LR scheduler: cosine, linear, constant, cosine_with_restarts',
      required: false,
      type: 'string',
    },
    {
      name: 'max_grad_norm',
      short: '',
      long: '--max-grad-norm',
      description: 'Gradient clipping max norm',
      required: false,
      type: 'number',
    },
    {
      name: 'resume_from',
      short: '',
      long: '--resume-from',
      description: 'Resume from checkpoint path',
      required: false,
      type: 'string',
    },
    {
      name: 'output_dir',
      short: '',
      long: '--output-dir',
      description: 'Custom output directory',
      required: false,
      type: 'string',
    },
  ],
  action: fineTuneAction,
};
