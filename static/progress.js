const labels = {
  waiting: "Waiting",
  checking: "Checking input",
  synthesizing: "Synthesizing",
  saving: "Saving",
  completed: "Completed",
  failed: "Failed",
};

function formatProgress(state) {
  let text = labels[state.status] ?? "Unknown status";
  if (state.status === "synthesizing") {
    text += `: ${state.completed_lines} / ${state.total_lines} lines`;
  }
  if (state.elapsed_seconds != null) {
    const minutes = Math.floor(state.elapsed_seconds / 60);
    const seconds = Math.floor(state.elapsed_seconds % 60);
    text += ` · Elapsed: ${minutes}m ${seconds}s`;
  }
  return text;
}

for (const element of document.querySelectorAll("[data-progress-id]")) {
  const id = encodeURIComponent(element.dataset.progressId);

  async function update() {
    try {
      const response = await fetch(`/progress/${id}`, {
        cache: "no-store",
        signal: AbortSignal.timeout(10000),
      });
      if (!response.ok) throw new Error("Unable to fetch progress");
      const state = await response.json();
      element.textContent = formatProgress(state);
    } catch {
      element.textContent = "";
    }
  }

  update();
}
