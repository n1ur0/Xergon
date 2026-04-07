import { NextRequest, NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

type ProposalStatus = "active" | "passed" | "failed" | "closed";

interface Proposal {
  id: string;
  title: string;
  description: string;
  author: string;
  createdAt: string;
  votingStartsAt: string;
  votingEndsAt: string;
  status: ProposalStatus;
  votesFor: number;
  votesAgainst: number;
  abstain: number;
  quorum: number;
  userVote?: "for" | "against" | "abstain" | null;
  category?: string;
}

interface VotingPower {
  ergBalance: number;
  xrgStaked: number;
  votingPower: number;
  delegations: number;
}

interface GovernanceResponse {
  proposals: Proposal[];
  votingPower: VotingPower;
}

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

const MOCK_PROPOSALS: Proposal[] = [
  {
    id: "xg-001",
    title: "Reduce inference fee to 0.001 ERG per 1K tokens",
    description: "This proposal suggests reducing the base inference fee from the current 0.002 ERG per 1K tokens to 0.001 ERG per 1K tokens to increase adoption and make AI inference more accessible to the broader community.\n\nRationale:\n- Lower fees will attract more users and developers\n- Increased volume may offset lower per-request margins\n- Competitive pricing compared to centralized alternatives\n- Aligns with Xergon's mission of democratizing AI",
    author: "3WxTQSY6VxEL3UdRp2Nxv5Tf1T9K7mMNcVv3",
    createdAt: "2026-03-15T10:00:00Z",
    votingStartsAt: "2026-03-20T00:00:00Z",
    votingEndsAt: "2026-04-20T00:00:00Z",
    status: "active",
    votesFor: 1247,
    votesAgainst: 389,
    abstain: 156,
    quorum: 2000,
    category: "protocol",
  },
  {
    id: "xg-002",
    title: "Add Llama 4 Scout to verified model list",
    description: "Proposal to add Meta's Llama 4 Scout model to the Xergon verified model list. This model has shown strong performance on benchmarks and has wide community support.\n\nThe model will be available through multiple providers initially, with quality monitoring in place.",
    author: "9kDBn3vsXha5QCYbHsYoXWbyvQ8LzMc4aSPB",
    createdAt: "2026-03-10T08:00:00Z",
    votingStartsAt: "2026-03-15T00:00:00Z",
    votingEndsAt: "2026-04-15T00:00:00Z",
    status: "active",
    votesFor: 892,
    votesAgainst: 45,
    abstain: 78,
    quorum: 1000,
    category: "model-listing",
  },
  {
    id: "xg-003",
    title: "Treasury allocation for GPU subsidy program",
    description: "Allocate 500 ERG from the treasury to subsidize GPU rentals for new providers joining the network during Q2 2026. This will help bootstrap provider capacity and ensure model availability.\n\nBreakdown:\n- 300 ERG for new provider onboarding subsidies\n- 150 ERG for regional expansion (South America, Africa)\n- 50 ERG for community education and documentation",
    author: "2eRxo7Xh2ZJQP8c5N6mPsYFya3GWfHk8vMzE",
    createdAt: "2026-02-28T12:00:00Z",
    votingStartsAt: "2026-03-05T00:00:00Z",
    votingEndsAt: "2026-03-25T00:00:00Z",
    status: "active",
    votesFor: 2103,
    votesAgainst: 187,
    abstain: 234,
    quorum: 2000,
    category: "treasury",
  },
  {
    id: "xg-004",
    title: "Implement provider reputation v2 system",
    description: "Upgrade the provider reputation system to v2, incorporating latency-weighted scoring, user satisfaction surveys, and automated quality checks.\n\nChanges from v1:\n- Add latency percentile scoring\n- Weight recent performance more heavily\n- Include user-reported quality metrics\n- Automatic probation for scores below threshold",
    author: "3WxTQSY6VxEL3UdRp2Nxv5Tf1T9K7mMNcVv3",
    createdAt: "2026-01-15T10:00:00Z",
    votingStartsAt: "2026-01-20T00:00:00Z",
    votingEndsAt: "2026-02-20T00:00:00Z",
    status: "passed",
    votesFor: 3456,
    votesAgainst: 123,
    abstain: 89,
    quorum: 2000,
    category: "protocol",
  },
  {
    id: "xg-005",
    title: "Increase minimum stake requirement for providers",
    description: "Proposed increase of minimum stake from 10 ERG to 25 ERG for new provider registrations. This aims to improve network quality and reduce Sybil attacks.",
    author: "9kDBn3vsXha5QCYbHsYoXWbyvQ8LzMc4aSPB",
    createdAt: "2025-12-01T10:00:00Z",
    votingStartsAt: "2025-12-05T00:00:00Z",
    votingEndsAt: "2026-01-05T00:00:00Z",
    status: "failed",
    votesFor: 567,
    votesAgainst: 1234,
    abstain: 89,
    quorum: 1500,
    category: "provider",
  },
  {
    id: "xg-006",
    title: "Establish community moderation framework",
    description: "Create a formal community moderation framework with elected moderators, clear guidelines, and an appeals process for content and provider disputes.",
    author: "2eRxo7Xh2ZJQP8c5N6mPsYFya3GWfHk8vMzE",
    createdAt: "2025-11-10T10:00:00Z",
    votingStartsAt: "2025-11-15T00:00:00Z",
    votingEndsAt: "2025-12-15T00:00:00Z",
    status: "closed",
    votesFor: 2345,
    votesAgainst: 678,
    abstain: 234,
    quorum: 2000,
    category: "general",
  },
];

const MOCK_VOTING_POWER: VotingPower = {
  ergBalance: 142.5678,
  xrgStaked: 50.0,
  votingPower: 192.5678,
  delegations: 3,
};

// ---------------------------------------------------------------------------
// GET /api/governance
// ---------------------------------------------------------------------------

export async function GET(request: NextRequest) {
  try {
    const { searchParams } = new URL(request.url);
    const section = searchParams.get("section");

    if (section === "voting-power") {
      return NextResponse.json(MOCK_VOTING_POWER);
    }

    const proposalId = searchParams.get("id");
    if (proposalId) {
      const proposal = MOCK_PROPOSALS.find((p) => p.id === proposalId);
      if (!proposal) {
        return NextResponse.json({ error: "Proposal not found" }, { status: 404 });
      }
      return NextResponse.json(proposal);
    }

    return NextResponse.json({
      proposals: MOCK_PROPOSALS,
      votingPower: MOCK_VOTING_POWER,
      degraded: true,
    });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Internal server error" },
      { status: 500 },
    );
  }
}

// ---------------------------------------------------------------------------
// POST /api/governance
// ---------------------------------------------------------------------------

export async function POST(request: NextRequest) {
  try {
    const body = await request.json();
    const { action, proposalId, vote, title, description, category } = body;

    if (action === "vote") {
      if (!proposalId || !vote) {
        return NextResponse.json({ error: "Missing proposalId or vote" }, { status: 400 });
      }
      if (!["for", "against", "abstain"].includes(vote)) {
        return NextResponse.json({ error: "Invalid vote" }, { status: 400 });
      }
      // In production this would verify wallet signature and record on-chain
      return NextResponse.json({ success: true, proposalId, vote });
    }

    if (action === "create") {
      if (!title || !description) {
        return NextResponse.json({ error: "Missing title or description" }, { status: 400 });
      }
      // In production this would verify wallet signature and create on-chain
      const newProposal: Proposal = {
        id: `xg-${String(MOCK_PROPOSALS.length + 1).padStart(3, "0")}`,
        title,
        description,
        author: "current_user",
        createdAt: new Date().toISOString(),
        votingStartsAt: new Date(Date.now() + 5 * 24 * 60 * 60 * 1000).toISOString(),
        votingEndsAt: new Date(Date.now() + 35 * 24 * 60 * 60 * 1000).toISOString(),
        status: "active",
        votesFor: 0,
        votesAgainst: 0,
        abstain: 0,
        quorum: 1000,
        category: category ?? "general",
      };
      return NextResponse.json({ success: true, proposal: newProposal });
    }

    return NextResponse.json({ error: "Invalid action" }, { status: 400 });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Internal server error" },
      { status: 500 },
    );
  }
}
