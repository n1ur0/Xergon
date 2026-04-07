import { NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface UserProfile {
  address: string;
  displayName: string;
  avatar?: string;
  bio?: string;
  joinedAt: string;
  stats: {
    totalSpentNanoErg: number;
    totalRequests: number;
    totalTokensConsumed: number;
    rentalCount: number;
    activeRentals: number;
    favoriteModels: string[];
    mostUsedProvider?: string;
  };
  reputation: {
    score: number;
    level: "bronze" | "silver" | "gold" | "platinum";
    disputesOpened: number;
    disputesResolved: number;
  };
  preferences: {
    defaultModel: string;
    preferredRegion: string;
    notificationsEnabled: boolean;
  };
}

// ---------------------------------------------------------------------------
// Deterministic mock data generator
// ---------------------------------------------------------------------------

function generateMockProfile(address: string): UserProfile {
  let seed = 0;
  for (let i = 0; i < address.length; i++)
    seed = (seed * 31 + address.charCodeAt(i)) | 0;

  function rand(): number {
    seed = (seed * 16807 + 12345) & 0x7fffffff;
    return seed / 0x7fffffff;
  }

  const models = [
    "llama-3.1-70b",
    "qwen2.5-72b",
    "mistral-7b",
    "deepseek-coder-33b",
    "phi-3-medium",
  ];

  const regions = [
    "North America",
    "Europe",
    "Asia",
    "South America",
    "Oceania",
  ];

  const joinedDate = new Date();
  joinedDate.setMonth(joinedDate.getMonth() - Math.floor(rand() * 12 + 1));

  const favoriteModels = models
    .sort(() => rand() - 0.5)
    .slice(0, Math.floor(rand() * 3 + 1));

  const reputationScore = Math.floor(rand() * 1000);
  const reputationLevel: UserProfile["reputation"]["level"] =
    reputationScore >= 800
      ? "platinum"
      : reputationScore >= 600
        ? "gold"
        : reputationScore >= 350
          ? "silver"
          : "bronze";

  return {
    address,
    displayName: address.length > 10 ? `User_${address.slice(0, 6)}` : address,
    bio:
      rand() > 0.5
        ? "AI inference enthusiast on the Xergon network."
        : undefined,
    joinedAt: joinedDate.toISOString(),
    stats: {
      totalSpentNanoErg: Math.floor(rand() * 500_000_000_000),
      totalRequests: Math.floor(rand() * 50_000),
      totalTokensConsumed: Math.floor(rand() * 10_000_000),
      rentalCount: Math.floor(rand() * 100),
      activeRentals: Math.floor(rand() * 5),
      favoriteModels,
      mostUsedProvider:
        rand() > 0.3
          ? `9${Math.random().toString(36).slice(2, 10)}...${Math.random().toString(36).slice(2, 6)}`
          : undefined,
    },
    reputation: {
      score: reputationScore,
      level: reputationLevel,
      disputesOpened: Math.floor(rand() * 3),
      disputesResolved: Math.floor(rand() * 3),
    },
    preferences: {
      defaultModel: favoriteModels[0],
      preferredRegion: regions[Math.floor(rand() * regions.length)],
      notificationsEnabled: rand() > 0.3,
    },
  };
}

// ---------------------------------------------------------------------------
// In-memory store (resets on server restart – fine for mock)
// ---------------------------------------------------------------------------

const profileStore = new Map<string, UserProfile>();

function getOrCreateProfile(address: string): UserProfile {
  let profile = profileStore.get(address);
  if (!profile) {
    profile = generateMockProfile(address);
    profileStore.set(address, profile);
  }
  return profile;
}

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

export async function GET(request: Request) {
  try {
    const { searchParams } = new URL(request.url);
    const address =
      searchParams.get("address") || "3WxTQSY6VxEL3UdRp2Nxv5Tf1T9K7mMNcVv3";

    const profile = getOrCreateProfile(address);
    return NextResponse.json(profile);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Internal server error" },
      { status: 500 },
    );
  }
}

// ---------------------------------------------------------------------------
// PATCH handler
// ---------------------------------------------------------------------------

export async function PATCH(request: Request) {
  try {
    const body = await request.json();
    const { address, displayName, bio, avatar, preferences } = body;

    if (!address) {
      return NextResponse.json(
        { error: "address is required" },
        { status: 400 },
      );
    }

    const profile = getOrCreateProfile(address);

    if (displayName !== undefined) profile.displayName = displayName;
    if (bio !== undefined) profile.bio = bio;
    if (avatar !== undefined) profile.avatar = avatar;
    if (preferences) {
      if (preferences.defaultModel !== undefined)
        profile.preferences.defaultModel = preferences.defaultModel;
      if (preferences.preferredRegion !== undefined)
        profile.preferences.preferredRegion = preferences.preferredRegion;
      if (preferences.notificationsEnabled !== undefined)
        profile.preferences.notificationsEnabled =
          preferences.notificationsEnabled;
    }

    profileStore.set(address, profile);

    return NextResponse.json({ success: true });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Internal server error" },
      { status: 500 },
    );
  }
}
