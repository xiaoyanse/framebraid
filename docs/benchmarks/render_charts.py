"""Render documentation charts from the checked-in sanitized observations only."""

import json
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.ticker import MaxNLocator


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
OUT = ROOT / "assets" / "benchmarks"
DATA = json.loads((HERE / "observations.json").read_text(encoding="utf-8"))
HISTORY = DATA["historicalRelease"]
GREEN = "#073F32"
SAGE = "#8BA28B"
IVORY = "#F8F6F0"
INK = "#22372F"


def style(axis):
    axis.set_facecolor(IVORY)
    axis.grid(axis="y", color="#DCE3DA", linewidth=0.7, zorder=0)
    axis.tick_params(colors=INK, labelsize=10)
    axis.spines[["top", "right"]].set_visible(False)
    axis.spines[["left", "bottom"]].set_color("#B8C6B7")
    axis.yaxis.set_major_locator(MaxNLocator(integer=True))


def reload_chart():
    rows = HISTORY["cycles"]
    rounds = [row["reload"] for row in rows]
    private = [row["privateBytes"] / (1024 * 1024) for row in rows]
    handles = [row["processHandles"] for row in rows]
    warmup = HISTORY["plateau"]["warmupCycles"]
    growth = HISTORY["plateau"]["privateGrowthBytes"] / (1024 * 1024)
    fig, axes = plt.subplots(2, 1, figsize=(12, 6.4), sharex=True,
                             gridspec_kw={"height_ratios": [2.4, 1]})
    fig.set_facecolor(IVORY)
    fig.suptitle("Historical SDK | 30 DLL reload cycles", x=0.09,
                 ha="left", color=GREEN, fontsize=21, fontweight="bold")
    for axis in axes:
        style(axis)
        axis.axvspan(0.5, warmup + 0.5, color=SAGE, alpha=0.18, zorder=0)
        axis.set_xlim(0.5, len(rows) + 0.5)
    axes[0].plot(rounds, private, color=GREEN, linewidth=2,
                 marker="o", markersize=4)
    axes[0].set_ylabel("Whole-process private memory (MiB)", color=INK)
    axes[0].set_ylim(30, 39)
    axes[0].text(0.03, 0.90, f"5 warm-up cycles | next 25 cycles: +{growth:.2f} MiB",
                 transform=axes[0].transAxes, color=GREEN, fontsize=11)
    axes[1].plot(rounds, handles, color=GREEN, linewidth=2,
                 marker="o", markersize=3)
    axes[1].set_ylabel("Process handles", color=INK)
    axes[1].set_ylim(155, 159)
    axes[1].set_xlabel("Reload cycle", color=INK)
    axes[1].text(0.03, 0.77, "157 handles every cycle | SDK workers after close: 0",
                 transform=axes[1].transAxes, color=GREEN, fontsize=10)
    axes[1].xaxis.set_major_locator(MaxNLocator(integer=True))
    fig.text(0.09, 0.025,
             "Pre-extraction Windows CPU test. Native runtimes may stay resident. "
             "Not a current FrameBraid Release or 24/7 guarantee.",
             color=INK, fontsize=9)
    fig.subplots_adjust(left=0.09, right=0.97, top=0.87, bottom=0.12, hspace=0.12)
    fig.savefig(OUT / "reload-stability.png", dpi=160, facecolor=IVORY)
    plt.close(fig)


def pressure_chart():
    pressure = HISTORY["pressure"]
    owners = pressure["balancedOwners"]
    counts = [pressure["accepted"], pressure["rejected"],
              owners["retain"], owners["release"]]
    labels = ["Accepted", "Rejected", "Retain", "Release"]
    fig, axis = plt.subplots(figsize=(12, 4.7))
    fig.set_facecolor(IVORY)
    style(axis)
    fig.suptitle("Historical SDK | Queue pressure & ownership", x=0.09,
                 ha="left", color=GREEN, fontsize=21, fontweight="bold")
    bars = axis.bar(labels, counts, color=[GREEN, "#A8AEA7", SAGE, GREEN],
                    width=0.54, zorder=3)
    axis.bar_label(bars, padding=6, color=INK, fontsize=15, fontweight="bold")
    axis.set_ylabel("Count in one synthetic CPU test", color=INK)
    axis.set_ylim(0, max(counts) * 1.30)
    axis.axvline(1.5, color="#C4CFC1", linestyle="--", linewidth=1)
    fig.text(0.09, 0.04,
             "37 accepted + 14 rejected; retained owners returned 37/37. "
             "Counts are not FPS, throughput or inference latency.",
             color=INK, fontsize=10)
    fig.subplots_adjust(left=0.09, right=0.97, top=0.82, bottom=0.18)
    fig.savefig(OUT / "queue-pressure.png", dpi=160, facecolor=IVORY)
    plt.close(fig)


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    reload_chart()
    pressure_chart()
    print("Rendered two charts from observations.json; no SDK/device tests executed.")
