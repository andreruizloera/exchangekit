import type { LeaderboardEntry, TierId } from "../types";

const KEY = "exchangekit.leaderboard.v1";
const PER_TIER = 5;

function read(): LeaderboardEntry[] {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as LeaderboardEntry[];
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function write(entries: LeaderboardEntry[]): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(entries));
  } catch {
    // storage disabled or full; the leaderboard is best-effort
  }
}

/** Best runs for a tier, highest return first. */
export function leaderboard(tier: TierId): LeaderboardEntry[] {
  return read()
    .filter((e) => e.tier === tier)
    .sort((a, b) => b.return_pct - a.return_pct)
    .slice(0, PER_TIER);
}

/** Record a finished run, keeping only the best few per tier. */
export function addLeaderboardEntry(entry: LeaderboardEntry): void {
  const all = read();
  all.push(entry);
  const kept: LeaderboardEntry[] = [];
  for (const tier of ["easy", "medium", "hard"] as TierId[]) {
    kept.push(
      ...all
        .filter((e) => e.tier === tier)
        .sort((a, b) => b.return_pct - a.return_pct)
        .slice(0, PER_TIER),
    );
  }
  write(kept);
}
