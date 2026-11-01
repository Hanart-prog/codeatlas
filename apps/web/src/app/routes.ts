import { BarChart3, FileText, GitCompareArrows, LayoutDashboard, type LucideIcon, Network, Radar } from "lucide-react";

export interface AppRoute {
  id: string;
  label: string;
  icon: LucideIcon;
  /** What the page will answer, shown until it is built. */
  purpose: string;
  /** The specification phase that delivers the page; absent once built. */
  plannedPhase?: number;
}

export const ROUTES: readonly [AppRoute, ...AppRoute[]] = [
  {
    id: "overview",
    label: "Overview",
    icon: LayoutDashboard,
    purpose: "Open a repository to see its history, coverage and most changed files.",
  },
  {
    id: "explorer",
    label: "Architecture Explorer",
    icon: Network,
    purpose: "Browse the import graph at any commit, with cycles and resolution coverage.",
  },
  {
    id: "impact",
    label: "Change Impact",
    icon: Radar,
    purpose: "Pick changed files and a historical cutoff to get a ranked, evidence-backed list of files to inspect.",
  },
  {
    id: "time-machine",
    label: "Time Machine",
    icon: GitCompareArrows,
    purpose: "Compare the import graph at two commits: added and removed files, edges and cycles.",
  },
  {
    id: "backtesting",
    label: "Backtesting",
    icon: BarChart3,
    purpose: "Measure recommendations against later commits and baselines, with sample sizes and known biases.",
  },
  {
    id: "reports",
    label: "Reports",
    icon: FileText,
    purpose: "Export results as JSON or static HTML with provenance and algorithm versions.",
    plannedPhase: 6,
  },
];
