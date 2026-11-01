import { BarChart3, FileText, Gauge, GitCompareArrows, LayoutDashboard, type LucideIcon, Network, Radar } from "lucide-react";

export interface AppRoute {
  id: string;
  label: string;
  icon: LucideIcon;
  /** Marker color of the icon, which tells sections apart at a glance. */
  tint: string;
  /** What the page answers. */
  purpose: string;
}

export const ROUTES: readonly [AppRoute, ...AppRoute[]] = [
  {
    id: "overview",
    label: "Overview",
    icon: LayoutDashboard,
    tint: "text-marker-azure",
    purpose: "Open a repository to see its history, coverage and most changed files.",
  },
  {
    id: "explorer",
    label: "Architecture Explorer",
    icon: Network,
    tint: "text-marker-teal",
    purpose: "Browse the import graph at any commit, with cycles and resolution coverage.",
  },
  {
    id: "impact",
    label: "Change Impact",
    icon: Radar,
    tint: "text-marker-coral",
    purpose: "Pick changed files and a historical cutoff to get a ranked, evidence-backed list of files to inspect.",
  },
  {
    id: "time-machine",
    label: "Time Machine",
    icon: GitCompareArrows,
    tint: "text-marker-orchid",
    purpose: "Compare the import graph at two commits: added and removed files, edges and cycles.",
  },
  {
    id: "backtesting",
    label: "Backtesting",
    icon: BarChart3,
    tint: "text-marker-amber",
    purpose: "Measure recommendations against later commits and baselines, with sample sizes and known biases.",
  },
  {
    id: "benchmarks",
    label: "Benchmarks",
    icon: Gauge,
    tint: "text-marker-teal",
    purpose: "See measured speed on generated and public repositories.",
  },
  {
    id: "reports",
    label: "Reports",
    icon: FileText,
    tint: "text-marker-azure",
    purpose: "Export results as JSON or static HTML with provenance and algorithm versions.",
  },
];
