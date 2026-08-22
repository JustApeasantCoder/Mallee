import { getVersion } from "@tauri-apps/api/app";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type FormEvent, type MouseEvent, type PointerEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import type { Action, Artifact, ProjectSummary, RunEvent, RunRecord } from "./types";
import malleeMark from "./assets/mallee-mark.svg";

type LayoutSplits = {
  left: number;
  actions: number;
  artifacts: number;
};

const LAYOUT_SPLITS_KEY = "mallee-desktop-layout-splits-v1";
const DEFAULT_LAYOUT_SPLITS: LayoutSplits = { left: 0.62, actions: 0.35, artifacts: 0.2 };

function loadLayoutSplits(): LayoutSplits {
  try {
    const saved = JSON.parse(window.localStorage.getItem(LAYOUT_SPLITS_KEY) ?? "null") as Partial<LayoutSplits> | null;
    if (!saved) return DEFAULT_LAYOUT_SPLITS;
    return {
      left: clampNumber(saved.left, 0.4, 0.72, DEFAULT_LAYOUT_SPLITS.left),
      actions: clampNumber(saved.actions, 0.22, 0.58, DEFAULT_LAYOUT_SPLITS.actions),
      artifacts: clampNumber(saved.artifacts, 0, 1, DEFAULT_LAYOUT_SPLITS.artifacts),
    };
  } catch {
    return DEFAULT_LAYOUT_SPLITS;
  }
}

function clampNumber(value: unknown, minimum: number, maximum: number, fallback: number) {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.min(maximum, Math.max(minimum, value))
    : fallback;
}

function actionGlyph(action: Pick<Action, "id" | "label" | "operation" | "icon">) {
  if (action.icon) return action.icon;
  // Action IDs are project-defined, so use their intent rather than requiring
  // every manifest to use one of a handful of exact IDs.
  const description = `${action.id} ${action.label} ${action.operation ?? ""}`
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ");

  if (/\b(open|folder|logs?)\b/.test(description)) return "↗";
  if (/\b(install|installer|msi|nsis|exe)\b/.test(description)) return "⇩";
  if (/\b(release|publish)\b/.test(description)) return "◆";
  if (/\b(build|compile|bundle)\b/.test(description)) return "◇";
  if (/\b(test|check|lint|verify)\b/.test(description)) return "✓";
  if (/\b(dev|frontend|serve|watch)\b/.test(description)) return ">_";
  if (/\b(run|start)\b/.test(description)) return "▷";
  return "□";
}

function ArtifactOpenIcon({ folder = false }: { folder?: boolean }) {
  return folder ? (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M3.5 6.5h6l1.8 2H20.5v9.8a1.7 1.7 0 0 1-1.7 1.7H5.2a1.7 1.7 0 0 1-1.7-1.7V8.2a1.7 1.7 0 0 1 1.7-1.7Z" />
      <path d="M13 12h5m0 0-2.2-2.2M18 12l-2.2 2.2" />
    </svg>
  ) : (
    <svg viewBox="0 0 24 24" aria-hidden="true">
      <path d="M13 3.5H6.2a1.7 1.7 0 0 0-1.7 1.7v13.6a1.7 1.7 0 0 0 1.7 1.7h11.6a1.7 1.7 0 0 0 1.7-1.7V10.2Z" />
      <path d="M13 3.5v6.7h6.5M9 15h6m0 0-2.2-2.2M15 15l-2.2 2.2" />
    </svg>
  );
}

type ProjectTerminalProps = {
  projectId: string;
  projectName: string;
  selected: boolean;
  onCopy: (contents: string) => void;
  onRegister: (projectId: string, terminal?: Terminal) => void;
};

function ProjectTerminal({ projectId, projectName, selected, onCopy, onRegister }: ProjectTerminalProps) {
  const host = useRef<HTMLDivElement>(null);
  const fit = useRef<FitAddon | undefined>(undefined);

  useEffect(() => {
    const element = host.current;
    if (!element) return;

    const instance = new Terminal({
      cursorBlink: false,
      convertEol: true,
      disableStdin: true,
      fontFamily: '"Cascadia Code", "SFMono-Regular", Consolas, monospace',
      fontSize: 13,
      lineHeight: 1.28,
      scrollback: 5000,
      theme: {
        background: "#080a0c",
        foreground: "#d6dbe0",
        cursor: "#9bd64b",
        black: "#080a0c",
        red: "#ff615c",
        green: "#9bd64b",
        yellow: "#e0b44c",
        blue: "#4c91ff",
      },
    });
    const fitAddon = new FitAddon();
    fit.current = fitAddon;
    instance.loadAddon(fitAddon);
    instance.open(element);
    instance.attachCustomKeyEventHandler((event) => {
      const isCopyShortcut = (event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === "c";
      if (event.type === "keydown" && isCopyShortcut && instance.hasSelection()) {
        event.preventDefault();
        onCopy(instance.getSelection());
        return false;
      }
      return true;
    });
    const resizeObserver = new ResizeObserver(() => fitAddon.fit());
    resizeObserver.observe(element);
    instance.writeln(`\x1b[34m# ${projectName} terminal\x1b[0m`);
    onRegister(projectId, instance);

    return () => {
      resizeObserver.disconnect();
      onRegister(projectId);
      instance.dispose();
    };
  }, [onCopy, onRegister, projectId, projectName]);

  useEffect(() => {
    if (selected) requestAnimationFrame(() => fit.current?.fit());
  }, [selected]);

  return <div className={`terminal-host ${selected ? "terminal-host-active" : "terminal-host-inactive"}`} ref={host} />;
}

function App() {
  const [appVersion, setAppVersion] = useState<string>();
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [projectIcons, setProjectIcons] = useState<Record<string, string>>({});
  const [selectedId, setSelectedId] = useState<string>();
  const [history, setHistory] = useState<RunRecord[]>([]);
  const [artifacts, setArtifacts] = useState<Artifact[]>([]);
  const [running, setRunning] = useState<Record<string, string>>({});
  const [error, setError] = useState<string>();
  const [showAddAction, setShowAddAction] = useState(false);
  const [actionMenu, setActionMenu] = useState<{ action: Action; x: number; y: number }>();
  const [editingAction, setEditingAction] = useState<Action>();
  const [dragging, setDragging] = useState<{ kind: "project" | "action"; id: string }>();
  const [layoutSplits, setLayoutSplits] = useState<LayoutSplits>(loadLayoutSplits);
  const [actionsContentHeight, setActionsContentHeight] = useState(150);

  useEffect(() => {
    void getVersion().then(setAppVersion).catch(() => undefined);
  }, []);
  const contentGrid = useRef<HTMLDivElement>(null);
  const actionsPanel = useRef<HTMLElement>(null);
  const actionGrid = useRef<HTMLDivElement>(null);
  const terminals = useRef(new Map<string, Terminal>());
  const selectedIdRef = useRef<string | undefined>(undefined);
  const dragMoved = useRef(false);

  const selected = useMemo(
    () => projects.find((project) => project.manifest.id === selectedId),
    [projects, selectedId],
  );

  useEffect(() => {
    selectedIdRef.current = selectedId;
  }, [selectedId]);

  useEffect(() => {
    window.localStorage.setItem(LAYOUT_SPLITS_KEY, JSON.stringify(layoutSplits));
  }, [layoutSplits]);

  useLayoutEffect(() => {
    const panel = actionsPanel.current;
    const grid = actionGrid.current;
    if (!panel || !grid) return;

    const measure = () => {
      const panelBounds = panel.getBoundingClientRect();
      const gridBounds = grid.getBoundingClientRect();
      // The Actions row must contain the title and every action tile.  Measuring
      // the rendered grid keeps this correct when cards wrap at narrower widths.
      const height = Math.ceil(gridBounds.bottom - panelBounds.top + 1);
      setActionsContentHeight((current) => current === height ? current : height);
    };

    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(grid);
    observer.observe(panel);
    return () => observer.disconnect();
  }, [selected?.manifest.actions]);

  const loadProjects = useCallback(async () => {
    try {
      const loaded = await invoke<ProjectSummary[]>("list_projects");
      setProjects(loaded);
      const icons = await Promise.all(loaded.map(async (project) => [
        project.manifest.id,
        await invoke<string | null>("project_icon", { projectId: project.manifest.id }),
      ] as const));
      setProjectIcons(Object.fromEntries(icons.filter(([, icon]) => icon)) as Record<string, string>);
      setSelectedId((current) => current ?? loaded[0]?.manifest.id);
      setError(undefined);
    } catch (reason) {
      setError(String(reason));
    }
  }, []);

  const loadProjectData = useCallback(async (projectId: string) => {
    const [runs, files] = await Promise.all([
      invoke<RunRecord[]>("get_history", { projectId }),
      invoke<Artifact[]>("get_artifacts", { projectId }),
    ]);
    setHistory(runs);
    setArtifacts(files);
  }, []);

  const copyTerminalText = useCallback(async (contents: string) => {
    if (!contents) return;

    try {
      await navigator.clipboard.writeText(contents);
    } catch (reason) {
      setError(`Unable to copy terminal output: ${String(reason)}`);
    }
  }, []);

  const registerTerminal = useCallback((projectId: string, instance?: Terminal) => {
    if (instance) terminals.current.set(projectId, instance);
    else terminals.current.delete(projectId);
  }, []);

  useEffect(() => {
    void loadProjects();
  }, [loadProjects]);

  useEffect(() => {
    if (!selectedId) return;
    setHistory([]);
    setArtifacts([]);
    void loadProjectData(selectedId).catch((reason) => setError(String(reason)));
  }, [loadProjectData, selectedId]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void listen<RunEvent>("mallee://run-event", (event) => {
      const payload = event.payload;
      setRunning((current) => {
        const next = { ...current };
        const key = runKey(payload.projectId, payload.actionId);
        if (payload.kind === "started") next[key] = payload.runId;
        if (payload.kind === "finished" && next[key] === payload.runId) delete next[key];
        return next;
      });
      const terminal = terminals.current.get(payload.projectId);
      if (payload.kind === "started") {
        terminal?.writeln(`\r\n\x1b[34m[mallee]\x1b[0m started ${payload.actionId}`);
      } else if (payload.kind === "output" && payload.line !== undefined) {
          // Many tools (including Cargo) use stderr for normal progress. Reserve
          // red for Mallee's actual run failures instead of the output stream.
        terminal?.writeln(payload.line);
      } else if (payload.kind === "finished") {
        if (payload.line) terminal?.writeln(`\x1b[31m${payload.line}\x1b[0m`);
        const color = payload.status === "success" ? "\x1b[32m" : "\x1b[31m";
        terminal?.writeln(
          `${color}[mallee] ${payload.status} (${formatDuration(payload.durationMs)})\x1b[0m`,
        );
        if (payload.projectId === selectedIdRef.current) {
          void loadProjectData(payload.projectId);
        }
      }
    }).then((dispose) => {
      if (disposed) {
        dispose();
      } else {
        unlisten = dispose;
      }
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [loadProjectData]);

  async function runAction(action: Action) {
    if (!selected) return;
    const confirmed = !action.confirm || window.confirm(`Run ${action.label} for ${selected.manifest.name}?`);
    if (!confirmed) return;
    setError(undefined);
    if (action.program) {
      const terminal = terminals.current.get(selected.manifest.id);
      terminal?.clear();
      terminal?.writeln(
        `\x1b[32m${selected.root}>\x1b[0m ${action.program} ${action.args.join(" ")}`,
      );
    }
    try {
      await invoke<string>("start_action", {
        projectId: selected.manifest.id,
        actionId: action.id,
        confirmed,
      });
    } catch (reason) {
      setError(String(reason));
      terminals.current.get(selected.manifest.id)?.writeln(`\x1b[31m[mallee] ${String(reason)}\x1b[0m`);
    }
  }

  async function stopAction(actionId: string) {
    if (!selected) return;
    const activeRunId = running[runKey(selected.manifest.id, actionId)];
    if (!activeRunId) return;
    try {
      await invoke("stop_run", { runId: activeRunId });
    } catch (reason) {
      setError(String(reason));
    }
  }

  function copyTerminal() {
    const instance = selectedId ? terminals.current.get(selectedId) : undefined;
    if (!instance) return;

    instance.selectAll();
    const contents = instance.getSelection();
    instance.clearSelection();
    void copyTerminalText(contents);
  }

  async function openArtifact(artifact: Artifact, location: "file" | "folder") {
    if (!selected) return;
    try {
      await invoke(location === "file" ? "open_artifact" : "open_artifact_folder", {
        projectId: selected.manifest.id,
        artifactPath: artifact.path,
      });
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function registerProject() {
    const path = window.prompt("Repository path containing .mallee/project.toml");
    if (!path) return;
    try {
      const project = await invoke<ProjectSummary>("add_project", { path });
      await loadProjects();
      setSelectedId(project.manifest.id);
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function createAction(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selected) return;
    const form = new FormData(event.currentTarget);
    const action: Action = {
      id: String(form.get("id") ?? "").trim(),
      label: String(form.get("label") ?? "").trim(),
      program: String(form.get("program") ?? "").trim(),
      args: String(form.get("args") ?? "")
        .split(/\r?\n/)
        .map((value) => value.trim())
        .filter(Boolean),
      kind: form.get("kind") === "long_running" ? "long_running" : "task",
      terminal: "captured",
      concurrency: form.get("kind") === "long_running" ? "replace_same_action" : "allow",
      confirm: form.get("confirm") === "on",
    };
    try {
      await invoke<ProjectSummary>("add_action", { projectId: selected.manifest.id, action });
      setShowAddAction(false);
      await loadProjects();
      setSelectedId(selected.manifest.id);
    } catch (reason) {
      setError(String(reason));
    }
  }

  function move<T extends { manifest?: { id: string }; id?: string }>(items: T[], sourceId: string, targetId: string) {
    const source = items.findIndex((item) => (item.manifest?.id ?? item.id) === sourceId);
    const target = items.findIndex((item) => (item.manifest?.id ?? item.id) === targetId);
    if (source < 0 || target < 0 || source === target) return items;
    const next = [...items];
    next.splice(target, 0, next.splice(source, 1)[0]);
    return next;
  }

  function beginDrag(event: PointerEvent<HTMLElement>, kind: "project" | "action", id: string) {
    if (event.button !== 0) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    dragMoved.current = false;
    setDragging({ kind, id });
  }

  function updateDrag(event: PointerEvent<HTMLElement>) {
    if (!dragging) return;
    const destination = document.elementFromPoint(event.clientX, event.clientY)?.closest<HTMLElement>(`[data-drag-${dragging.kind}]`)?.dataset.dragId;
    if (!destination || destination === dragging.id) return;
    dragMoved.current = true;
    if (dragging.kind === "project") {
      setProjects((current) => move(current, dragging.id, destination));
    } else if (selected) {
      setProjects((current) => current.map((project) => project.manifest.id === selected.manifest.id
        ? { ...project, manifest: { ...project.manifest, actions: move(project.manifest.actions, dragging.id, destination) } }
        : project));
    }
  }

  async function finishDrag() {
    if (!dragging) return;
    const completed = dragging;
    setDragging(undefined);
    try {
      if (completed.kind === "project") {
        await invoke("reorder_projects", { projectIds: projects.map((project) => project.manifest.id) });
      } else if (selected) {
        const current = projects.find((project) => project.manifest.id === selected.manifest.id);
        if (current) await invoke("reorder_actions", { projectId: selected.manifest.id, actionIds: current.manifest.actions.map((action) => action.id) });
      }
    } catch (reason) {
      setError(String(reason));
      await loadProjects();
    }
  }

  async function deleteAction(action: Action) {
    if (!selected || !window.confirm(`Remove ${action.label}? This removes it from .mallee/project.toml.`)) return;
    try {
      await invoke("remove_action", { projectId: selected.manifest.id, actionId: action.id });
      await loadProjects();
    } catch (reason) {
      setError(String(reason));
    }
  }

  function openActionMenu(event: MouseEvent<HTMLDivElement>, action: Action) {
    event.preventDefault();
    event.stopPropagation();
    setActionMenu({ action, x: event.clientX, y: event.clientY });
  }

  async function saveActionPresentation(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selected || !editingAction) return;
    const form = new FormData(event.currentTarget);
    const label = String(form.get("label") ?? "").trim();
    const icon = String(form.get("icon") ?? "").trim() || undefined;
    if (!label) return;
    try {
      await invoke("update_action_presentation", {
        projectId: selected.manifest.id,
        actionId: editingAction.id,
        label,
        icon,
      });
      setEditingAction(undefined);
      await loadProjects();
    } catch (reason) {
      setError(String(reason));
    }
  }

  function beginResize(event: PointerEvent<HTMLDivElement>, axis: keyof LayoutSplits) {
    if (event.button !== 0) return;
    const grid = contentGrid.current;
    if (!grid) return;

    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    const bounds = grid.getBoundingClientRect();
    const usableWidth = Math.max(1, bounds.width - 10);
    const usableHeight = Math.max(1, bounds.height - 20);

    const resize = (clientX: number, clientY: number) => {
      setLayoutSplits((current) => {
        if (axis === "left") {
          return { ...current, left: Math.min(0.72, Math.max(0.4, (clientX - bounds.left) / usableWidth)) };
        }

        if (axis === "actions") {
          return { ...current, actions: Math.min(0.58, Math.max(0.22, (clientY - bounds.top) / usableHeight)) };
        }

        return { ...current, artifacts: Math.max(0, (bounds.bottom - clientY) / usableHeight) };
      });
    };

    const onMove = (moveEvent: globalThis.PointerEvent) => resize(moveEvent.clientX, moveEvent.clientY);
    const finish = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", finish);
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <header className="brand"><img className="brand-mark" src={malleeMark} alt="" />MALLEE</header>
        <div className="side-label">PROJECTS</div>
        <nav className="project-list">
          {projects.map((project) => (
            <button
              className={`project-item ${selectedId === project.manifest.id ? "selected" : ""}`}
              key={project.manifest.id}
              onClick={() => setSelectedId(project.manifest.id)}
              data-drag-project
              data-drag-id={project.manifest.id}
              onPointerDown={(event) => beginDrag(event, "project", project.manifest.id)}
              onPointerMove={updateDrag}
              onPointerUp={() => void finishDrag()}
              onPointerCancel={() => setDragging(undefined)}
            >
              {projectIcons[project.manifest.id] ? (
                <img className="project-icon-image" src={projectIcons[project.manifest.id]} alt="" />
              ) : (
                <span className="project-icon" aria-hidden="true">▦</span>
              )}
              <span className="project-name">{project.manifest.name}</span>
              <span className={`state-dot ${Object.keys(running).some((key) => key.startsWith(`${project.manifest.id}:`)) ? "active" : ""}`} />
            </button>
          ))}
        </nav>
        <div className="sidebar-footer">
          <button className="side-action" onClick={registerProject}>＋ Add Project</button>
          <span className="version">{appVersion ? `v${appVersion}` : ""}</span>
        </div>
      </aside>

      <main className="workspace">
        {selected ? (
          <>
            <header className="project-header">
              <div>
                <h1>{selected.manifest.name}</h1>
                <p>{selected.root}</p>
              </div>
              {selected.manifest.logs.open_with_deebugee && (
                <div className="header-actions">
                  <button onClick={() => void invoke("open_in_deebugee", { projectId: selected.manifest.id }).catch((reason) => setError(String(reason)))}>
                    Open in DeeBugee ↗
                  </button>
                </div>
              )}
            </header>

            {error && <div className="error-banner"><span>{error}</span><button onClick={() => setError(undefined)}>×</button></div>}

            <div
              className="content-grid"
              ref={contentGrid}
              style={{
                "--left-split": layoutSplits.left,
                "--actions-split": layoutSplits.actions,
                "--artifacts-split": layoutSplits.artifacts,
                "--actions-content-height": `${actionsContentHeight}px`,
              } as CSSProperties}
            >
              <section className="panel actions-panel" ref={actionsPanel}>
                <div className="panel-title-row">
                  <h2>ACTIONS</h2>
                  <div>
                    <button className="link-button" onClick={() => setShowAddAction(true)}>＋ ADD ACTION</button>
                    <button className="link-button" onClick={() => void invoke("open_manifest", { projectId: selected.manifest.id }).catch((reason) => setError(String(reason)))}>↗ OPEN TOML</button>
                  </div>
                </div>
                <div className="action-grid" ref={actionGrid}>
                  {selected.manifest.actions.map((action) => {
                    const runId = running[runKey(selected.manifest.id, action.id)];
                    const latest = history.find((run) => run.actionId === action.id);
                    return (
                      <div className={`action-tile ${dragging?.kind === "action" && dragging.id === action.id ? "dragging" : ""}`} key={action.id}
                        data-drag-action data-drag-id={action.id}
                        onPointerDown={(event) => beginDrag(event, "action", action.id)} onPointerMove={updateDrag}
                        onPointerUp={() => void finishDrag()} onPointerCancel={() => setDragging(undefined)}
                        role="button" tabIndex={0}
                        onContextMenu={(event) => openActionMenu(event, action)}
                        onClick={() => { if (dragMoved.current) { dragMoved.current = false; return; } void (runId ? stopAction(action.id) : runAction(action)); }}
                        onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); void (runId ? stopAction(action.id) : runAction(action)); } }}>
                        <span className="action-glyph">{actionGlyph(action)}</span>
                        <span className="action-copy">
                          <strong>{runId ? `STOP ${action.label}` : action.label}</strong>
                          <small>{action.program ? `${action.program} ${action.args.join(" ")}` : action.operation}</small>
                        </span>
                        <span className={`latest-state ${runId ? "running" : latest?.status ?? "never"}`}>
                          {runId ? "RUNNING" : latest?.status?.toUpperCase() ?? "NEVER"}
                        </span>
                        <button className="action-remove" aria-label={`Remove ${action.label}`} title="Remove action"
                          onPointerDown={(event) => event.stopPropagation()}
                          onClick={(event) => { event.stopPropagation(); void deleteAction(action); }}>×</button>
                      </div>
                    );
                  })}
                </div>
              </section>

              <section className="panel history-panel">
                <div className="panel-title-row"><h2>HISTORY</h2><span>{history.length} RUNS</span></div>
                <div className="history-table">
                  {history.length === 0 && <div className="empty-row">No runs recorded for this project.</div>}
                  {history.map((run) => (
                    <div className="history-row" key={run.id}>
                      <div><strong>{run.actionLabel}</strong><small>{formatDate(run.startedAt)}</small></div>
                      <span>{formatDuration(run.durationMs)}</span>
                      <span className={`result ${run.status}`}>{run.status.toUpperCase()}</span>
                      <button title="Run again" onClick={() => {
                        const action = selected.manifest.actions.find((item) => item.id === run.actionId);
                        if (action) void runAction(action);
                      }}>▷</button>
                    </div>
                  ))}
                </div>
              </section>

              <section className="panel terminal-panel">
                <div className="panel-title-row">
                  <h2>CURRENT TERMINAL</h2>
                  <div className="terminal-actions">
                    <button className="link-button" onClick={() => void copyTerminal()}>COPY</button>
                    <button className="link-button" onClick={() => selectedId && terminals.current.get(selectedId)?.clear()}>CLEAR</button>
                  </div>
                </div>
                {projects.map((project) => (
                  <ProjectTerminal
                    key={project.manifest.id}
                    projectId={project.manifest.id}
                    projectName={project.manifest.name}
                    selected={project.manifest.id === selectedId}
                    onCopy={copyTerminalText}
                    onRegister={registerTerminal}
                  />
                ))}
              </section>

              <section className="panel artifacts-panel">
                <div className="panel-title-row"><h2>ARTIFACTS</h2><span>{artifacts.length} FILES</span></div>
                <div className="artifact-table">
                  <div className="artifact-head"><span>NAME</span><span>SIZE</span><span>MODIFIED</span><span className="artifact-actions-label">ACTIONS</span></div>
                  {artifacts.length === 0 && <div className="empty-row">No matching artifacts yet.</div>}
                  {artifacts.slice(0, 8).map((artifact) => (
                    <div className="artifact-row" key={artifact.path}>
                      <span title={artifact.path}>{artifact.name}</span>
                      <span>{formatBytes(artifact.size)}</span>
                      <span>{new Date(artifact.modifiedMs).toLocaleString()}</span>
                      <span className="artifact-actions">
                        <button className="icon-button" title="Open" aria-label={`Open ${artifact.name}`} onClick={() => void openArtifact(artifact, "file")}>
                          <ArtifactOpenIcon />
                        </button>
                        <button className="icon-button" title="Open Folder" aria-label={`Open folder containing ${artifact.name}`} onClick={() => void openArtifact(artifact, "folder")}>
                          <ArtifactOpenIcon folder />
                        </button>
                      </span>
                    </div>
                  ))}
                </div>
              </section>
              <div className="panel-resizer vertical-resizer" role="separator" aria-orientation="vertical" aria-label="Resize main columns" onPointerDown={(event) => beginResize(event, "left")} />
              <div className="panel-resizer actions-resizer" role="separator" aria-orientation="horizontal" aria-label="Resize actions and terminal" onPointerDown={(event) => beginResize(event, "actions")} />
              <div className="panel-resizer artifacts-resizer" role="separator" aria-orientation="horizontal" aria-label="Resize history and artifacts" onPointerDown={(event) => beginResize(event, "artifacts")} />
            </div>
          </>
        ) : (
          <div className="empty-workspace">
            <img className="brand-mark large" src={malleeMark} alt="" />
            <h1>No projects registered</h1>
            <p>Add a repository containing a valid .mallee/project.toml manifest.</p>
            <button onClick={registerProject}>Add Project</button>
          </div>
        )}
      </main>
      {showAddAction && selected && (
        <div className="modal-backdrop" onMouseDown={() => setShowAddAction(false)}>
          <form className="action-modal" onSubmit={createAction} onMouseDown={(event) => event.stopPropagation()}>
            <div className="modal-title"><div><span>NEW PROJECT ACTION</span><strong>{selected.manifest.name}</strong></div><button type="button" onClick={() => setShowAddAction(false)}>×</button></div>
            <label>Action ID<input name="id" required pattern="[a-z0-9-]+" placeholder="build-installer" /></label>
            <label>Label<input name="label" required placeholder="Build Installer" /></label>
            <label>Program<input name="program" required placeholder="pwsh" /></label>
            <label>Arguments <small>one argument per line</small><textarea name="args" rows={5} placeholder={'-NoProfile\n-File\n.mallee/scripts/build-installer.ps1'} /></label>
            <label>Behavior<select name="kind" defaultValue="task"><option value="task">Task — exits when complete</option><option value="long_running">Long running — stays active</option></select></label>
            <label className="check-label"><input name="confirm" type="checkbox" /> Require confirmation before running</label>
            <div className="manifest-preview">Writes a validated action to <strong>.mallee/project.toml</strong>. A backup is created first.</div>
            <div className="modal-actions"><button type="button" onClick={() => setShowAddAction(false)}>CANCEL</button><button className="primary" type="submit">ADD ACTION</button></div>
          </form>
        </div>
      )}
      {actionMenu && (
        <div className="context-menu-layer" onMouseDown={() => setActionMenu(undefined)}>
          <div className="action-context-menu" style={{ left: actionMenu.x, top: actionMenu.y }} onMouseDown={(event) => event.stopPropagation()}>
            <button onClick={() => { setEditingAction(actionMenu.action); setActionMenu(undefined); }}>Edit title…</button>
            <button onClick={() => { setEditingAction(actionMenu.action); setActionMenu(undefined); }}>Change icon…</button>
          </div>
        </div>
      )}
      {editingAction && selected && (
        <div className="modal-backdrop" onMouseDown={() => setEditingAction(undefined)}>
          <form className="action-modal" onSubmit={saveActionPresentation} onMouseDown={(event) => event.stopPropagation()}>
            <div className="modal-title"><div><span>EDIT ACTION CARD</span><strong>{editingAction.id}</strong></div><button type="button" onClick={() => setEditingAction(undefined)}>×</button></div>
            <label>Title<input name="label" required defaultValue={editingAction.label} autoFocus /></label>
            <label>Icon<select name="icon" defaultValue={editingAction.icon ?? ""}>
              <option value="">□ Automatic</option>
              <option value="▷">▷ Run</option><option value="✓">✓ Check</option><option value="◇">◇ Build</option>
              <option value="⇩">⇩ Install</option><option value="◆">◆ Release</option><option value="↗">↗ Open</option>
              <option value=">_">&gt;_ Development</option><option value="□">□ Generic</option>
            </select></label>
            <div className="manifest-preview">The title and icon are saved to <strong>.mallee/project.toml</strong>.</div>
            <div className="modal-actions"><button type="button" onClick={() => setEditingAction(undefined)}>CANCEL</button><button className="primary" type="submit">SAVE</button></div>
          </form>
        </div>
      )}
    </div>
  );
}

function formatDuration(value?: number) {
  if (value === undefined || value === null) return "—";
  if (value < 1000) return `${value} ms`;
  const seconds = Math.floor(value / 1000);
  if (seconds < 60) return `${seconds}s`;
  return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
}

function runKey(projectId: string, actionId: string) {
  return `${projectId}:${actionId}`;
}

function formatDate(value: string) {
  return new Date(value).toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

function formatBytes(value: number) {
  if (value < 1024) return `${value} B`;
  if (value < 1024 ** 2) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 ** 2).toFixed(1)} MB`;
}

export default App;
