function cloneStatus(st) {
  return {
    ...st,
    changedFiles: (st.changedFiles || []).map((f) => ({ ...f })),
    stagedFiles: (st.stagedFiles || []).map((f) => ({ ...f })),
    unstagedFiles: (st.unstagedFiles || []).map((f) => ({ ...f })),
  };
}

function rebuild(st) {
  const byPath = new Map();
  for (const f of st.stagedFiles) {
    byPath.set(f.path, f);
  }
  for (const f of st.unstagedFiles) {
    if (!byPath.has(f.path)) {
      byPath.set(f.path, f);
    }
  }
  st.changedFiles = Array.from(byPath.values());
  st.staged = st.stagedFiles.length;
  st.unstaged = st.unstagedFiles.length;
  return st;
}

export function stage(st, paths) {
  const next = cloneStatus(st);
  const set = new Set(paths);
  const moved = [];
  const rest = [];
  for (const f of next.unstagedFiles) {
    if (set.has(f.path)) {
      moved.push(f);
    } else {
      rest.push(f);
    }
  }
  next.unstagedFiles = rest;
  const staged = new Map(next.stagedFiles.map((f) => [f.path, f]));
  for (const f of moved) {
    if (String(f.status).toUpperCase() === 'U') {
      next.untracked = Math.max(0, (next.untracked || 0) - 1);
    }
    staged.set(f.path, { path: f.path, status: f.status });
  }
  next.stagedFiles = Array.from(staged.values());
  return rebuild(next);
}

export function unstage(st, paths) {
  const next = cloneStatus(st);
  const set = new Set(paths);
  const moved = [];
  const rest = [];
  for (const f of next.stagedFiles) {
    if (set.has(f.path)) {
      moved.push(f);
    } else {
      rest.push(f);
    }
  }
  next.stagedFiles = rest;
  const unstaged = new Map(next.unstagedFiles.map((f) => [f.path, f]));
  for (const f of moved) {
    const wasUntracked = String(f.status).toUpperCase() === 'A';
    unstaged.set(f.path, {
      path: f.path,
      status: wasUntracked ? 'U' : f.status,
    });
    if (wasUntracked) {
      next.untracked = (next.untracked || 0) + 1;
    }
  }
  next.unstagedFiles = Array.from(unstaged.values());
  return rebuild(next);
}

export function discard(st, paths) {
  const next = cloneStatus(st);
  const set = new Set(paths);
  const rest = [];
  for (const f of next.unstagedFiles) {
    if (set.has(f.path)) {
      if (String(f.status).toUpperCase() === 'U') {
        next.untracked = Math.max(0, (next.untracked || 0) - 1);
      }
    } else {
      rest.push(f);
    }
  }
  next.unstagedFiles = rest;
  return rebuild(next);
}

export function commit(st) {
  const next = cloneStatus(st);
  const committed = next.stagedFiles.length;
  next.stagedFiles = [];
  next.staged = 0;
  if (committed > 0 && next.isRepo !== false) {
    next.ahead = (next.ahead || 0) + 1;
  }
  return rebuild(next);
}

export function push(st) {
  const next = cloneStatus(st);
  next.ahead = 0;
  return next;
}