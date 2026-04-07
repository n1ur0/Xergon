import { NextRequest, NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface BillingOverview {
  totalSpent: number;
  thisMonth: number;
  invoiceCount: number;
  creditsRemaining: number;
}

interface Transaction {
  id: string;
  date: string;
  model: string;
  provider: string;
  promptTokens: number;
  completionTokens: number;
  cost: number;
  status: "completed" | "failed" | "refunded";
}

interface Invoice {
  id: string;
  amount: number;
  date: string;
  dueDate: string;
  status: "paid" | "pending" | "overdue" | "refunded";
  description: string;
}

interface UsageByModel {
  model: string;
  tokens: number;
  cost: number;
  requests: number;
}

interface SpendingPoint {
  date: string;
  amount: number;
}

interface BillingResponse {
  overview: BillingOverview;
  transactions: Transaction[];
  invoices: Invoice[];
  usageByModel: UsageByModel[];
  spendingChart: SpendingPoint[];
}

// ---------------------------------------------------------------------------
// Deterministic mock data
// ---------------------------------------------------------------------------

function seededRandom(seed: number) {
  let s = seed;
  return () => {
    s = (s * 16807 + 12345) & 0x7fffffff;
    return s / 0x7fffffff;
  };
}

function generateMockBilling(address?: string): BillingResponse {
  const now = new Date();
  const days = 30;

  let seed = 42;
  if (address) {
    for (let i = 0; i < address.length; i++) {
      seed = (seed * 31 + address.charCodeAt(i)) | 0;
    }
  }
  const rand = seededRandom(seed);

  const models = [
    "llama-3.1-70b",
    "qwen2.5-72b",
    "mistral-7b",
    "deepseek-coder-33b",
    "phi-3-medium",
    "codellama-34b",
  ];

  const providers = [
    "3WxTQSY6VxEL3UdRp2Nxv5Tf1T9K7mMNcVv3",
    "9kDBn3vsXha5QCYbHsYoXWbyvQ8LzMc4aSPB",
    "2eRxo7Xh2ZJQP8c5N6mPsYFya3GWfHk8vMzE",
  ];

  // Spending chart
  const spendingChart: SpendingPoint[] = Array.from({ length: days }, (_, i) => {
    const date = new Date(now);
    date.setDate(date.getDate() - (days - 1 - i));
    const dayOfWeek = date.getDay();
    const weekendFactor = dayOfWeek === 0 || dayOfWeek === 6 ? 0.6 : 1;
    const base = 0.5 + rand() * 2.5;
    return {
      date: date.toISOString().split("T")[0],
      amount: Math.round(base * weekendFactor * 10000) / 10000,
    };
  });

  const totalSpent = spendingChart.reduce((s, d) => s + d.amount, 0);

  // This month
  const thisMonthStart = new Date(now.getFullYear(), now.getMonth(), 1);
  const thisMonth = spendingChart
    .filter((d) => new Date(d.date + "T00:00:00") >= thisMonthStart)
    .reduce((s, d) => s + d.amount, 0);

  // Transactions
  const transactionCount = 50;
  const transactions: Transaction[] = Array.from({ length: transactionCount }, (_, i) => {
    const date = new Date(now);
    date.setMinutes(date.getMinutes() - i * 47);
    const model = models[Math.floor(rand() * models.length)];
    const provider = providers[Math.floor(rand() * providers.length)];
    const promptTokens = Math.floor(100 + rand() * 4000);
    const completionTokens = Math.floor(50 + rand() * 2000);
    const cost = (promptTokens * 0.000002 + completionTokens * 0.000004 + rand() * 0.001);
    const statuses: Array<"completed" | "failed" | "refunded"> = ["completed", "completed", "completed", "completed", "failed", "refunded"];
    const status = statuses[Math.floor(rand() * statuses.length)];

    return {
      id: `tx-${String(i + 1).padStart(4, "0")}`,
      date: date.toISOString(),
      model,
      provider,
      promptTokens,
      completionTokens,
      cost: Math.round(cost * 1e6) / 1e6,
      status,
    };
  });

  // Usage by model
  const usageByModel: UsageByModel[] = models.map((model) => {
    const modelTxs = transactions.filter((t) => t.model === model);
    const tokens = modelTxs.reduce((s, t) => s + t.promptTokens + t.completionTokens, 0);
    const cost = modelTxs.reduce((s, t) => s + t.cost, 0);
    return { model, tokens, cost: Math.round(cost * 1e6) / 1e6, requests: modelTxs.length };
  }).sort((a, b) => b.cost - a.cost);

  // Invoices
  const invoices: Invoice[] = Array.from({ length: 6 }, (_, i) => {
    const date = new Date(now);
    date.setMonth(date.getMonth() - i);
    const dueDate = new Date(date);
    dueDate.setDate(dueDate.getDate() + 30);
    const statuses: Array<"paid" | "pending" | "overdue" | "refunded"> = ["paid", "paid", "paid", "pending", "paid", "overdue"];
    return {
      id: `INV-${String(date.getFullYear()).slice(2)}${String(date.getMonth() + 1).padStart(2, "0")}${String(i + 1).padStart(3, "0")}`,
      amount: Math.round((3 + rand() * 20) * 10000) / 10000,
      date: date.toISOString().split("T")[0],
      dueDate: dueDate.toISOString().split("T")[0],
      status: statuses[i],
      description: `Xergon marketplace usage - ${date.toLocaleDateString("en-US", { month: "long", year: "numeric" })}`,
    };
  });

  return {
    overview: {
      totalSpent: Math.round(totalSpent * 10000) / 10000,
      thisMonth: Math.round(thisMonth * 10000) / 10000,
      invoiceCount: invoices.length,
      creditsRemaining: Math.round((10 + rand() * 40) * 10000) / 10000,
    },
    transactions,
    invoices,
    usageByModel,
    spendingChart,
  };
}

// ---------------------------------------------------------------------------
// GET /api/billing
// ---------------------------------------------------------------------------

export async function GET(request: NextRequest) {
  try {
    const { searchParams } = new URL(request.url);
    const section = searchParams.get("section"); // overview | transactions | invoices | usage

    const data = generateMockBilling(searchParams.get("address") ?? undefined);

    if (section === "overview") {
      return NextResponse.json(data.overview);
    }
    if (section === "transactions") {
      const page = parseInt(searchParams.get("page") ?? "1", 10);
      const limit = parseInt(searchParams.get("limit") ?? "20", 10);
      const start = (page - 1) * limit;
      return NextResponse.json({
        data: data.transactions.slice(start, start + limit),
        total: data.transactions.length,
        page,
        limit,
      });
    }
    if (section === "invoices") {
      return NextResponse.json(data.invoices);
    }
    if (section === "usage") {
      return NextResponse.json(data.usageByModel);
    }

    return NextResponse.json({ ...data, degraded: true });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Internal server error" },
      { status: 500 },
    );
  }
}
