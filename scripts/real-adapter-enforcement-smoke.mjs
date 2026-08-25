#!/usr/bin/env node

import { spawn } from "node:child_process";
import { once } from "node:events";
import { access, mkdtemp, mkdir, readFile, rm } from "node:fs/promises";
import net from "node:net";
import path from "node:path";
import process from "node:process";

const agentID = process.argv[2];
if (!new Set(["codex", "claude"]).has(agentID)) {
  console.error("usage: scripts/real-adapter-enforcement-smoke.mjs <codex|claude>");
  process.exit(2);
}

const repoRoot = path.resolve(import.meta.dirname, "..");
const corePath = path.join(repoRoot, "core", "target", "debug", "vela-core");
const root = await mkdtemp("/private/tmp/vela-real-enforcement-");
const workspace = path.join(root, "workspace");
const target = path.join(workspace, `${agentID}-must-not-exist.txt`);
const socketPath = path.join(root, "vela.sock");
await mkdir(workspace);

let core;
let socket;
let nextRequest = 1;
const pending = new Map();
const events = [];
const eventWaiters = [];
const coreErrors = [];

function withTimeout(promise, milliseconds, label) {
  let timer;
  return Promise.race([
    promise.finally(() => clearTimeout(timer)),
    new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(`${label} timed out after ${milliseconds}ms`)), milliseconds);
    }),
  ]);
}

function acceptEvent(message) {
  const waiterIndex = eventWaiters.findIndex(({ predicate }) => predicate(message));
  if (waiterIndex >= 0) {
    const [{ resolve }] = eventWaiters.splice(waiterIndex, 1);
    resolve(message);
  } else {
    events.push(message);
  }
}

function waitForEvent(predicate, milliseconds = 180_000) {
  const queuedIndex = events.findIndex(predicate);
  if (queuedIndex >= 0) {
    return Promise.resolve(events.splice(queuedIndex, 1)[0]);
  }
  return withTimeout(
    new Promise((resolve) => eventWaiters.push({ predicate, resolve })),
    milliseconds,
    "agent event",
  );
}

function request(method, params = {}, milliseconds = 30_000) {
  const id = `smoke-${nextRequest++}`;
  const response = withTimeout(
    new Promise((resolve, reject) => pending.set(id, { resolve, reject })),
    milliseconds,
    method,
  );
  socket.write(`${JSON.stringify({ version: { major: 1, minor: 0 }, id, method, params })}\n`);
  return response;
}

async function stopCore() {
  socket?.destroy();
  if (core && core.exitCode === null) {
    core.kill("SIGTERM");
    await Promise.race([once(core, "exit"), new Promise((resolve) => setTimeout(resolve, 2_000))]);
    if (core.exitCode === null) core.kill("SIGKILL");
  }
}

try {
  await access(corePath);
  core = spawn(corePath, ["--socket", socketPath], {
    cwd: repoRoot,
    stdio: ["ignore", "ignore", "pipe"],
  });
  core.stderr.setEncoding("utf8");
  core.stderr.on("data", (chunk) => {
    coreErrors.push(...chunk.trim().split("\n").filter(Boolean));
    if (coreErrors.length > 40) coreErrors.splice(0, coreErrors.length - 40);
  });

  for (let attempt = 0; attempt < 200; attempt += 1) {
    try {
      await access(socketPath);
      break;
    } catch {
      if (core.exitCode !== null) throw new Error(`vela-core exited with ${core.exitCode}`);
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
  }

  socket = net.createConnection(socketPath);
  await withTimeout(once(socket, "connect"), 5_000, "IPC connect");
  socket.setEncoding("utf8");
  let buffer = "";
  socket.on("data", (chunk) => {
    buffer += chunk;
    while (buffer.includes("\n")) {
      const newline = buffer.indexOf("\n");
      const line = buffer.slice(0, newline);
      buffer = buffer.slice(newline + 1);
      if (!line) continue;
      const message = JSON.parse(line);
      if (message.id && pending.has(message.id)) {
        const { resolve, reject } = pending.get(message.id);
        pending.delete(message.id);
        if (message.error) reject(new Error(`${message.error.code}: ${message.error.message}`));
        else resolve(message.result);
      } else if (message.event === "agent.event") {
        acceptEvent(message.data);
      }
    }
  });

  await request("core.hello");
  const registry = await request("agents.refresh", {}, 20_000);
  const agent = registry.agents.find(({ id }) => id === agentID);
  if (!agent) throw new Error(`discovery did not return ${agentID}`);
  if (agent.status !== "ready") {
    throw new Error(`${agentID} is ${agent.status}: ${agent.diagnostic ?? "no diagnostic"}`);
  }

  const session = await request(
    "session.create",
    { agent_id: agentID, cwd: workspace },
    60_000,
  );
  const prompt = [
    "This is a permission-enforcement smoke test in a disposable workspace.",
    `Use your native file edit/write tool (not a shell command) to create exactly ${target}`,
    "with the text VELA_REAL_ADAPTER_ENFORCEMENT. Do not perform any other action.",
  ].join(" ");
  const accepted = await request(
    "session.prompt",
    { session_id: session.id, text: prompt },
    30_000,
  );

  let permission;
  let terminal;
  const observedEvents = [];
  while (!terminal) {
    const event = await waitForEvent((candidate) => candidate.run_id === accepted.run_id);
    observedEvents.push(event);
    if (event.kind === "permission_requested") {
      if (permission) throw new Error("adapter emitted more than one permission request");
      permission = event.request;
      await request("permission.resolve", {
        permission_id: permission.id,
        session_id: permission.session_id,
        run_id: permission.run_id,
        decision: "deny",
      });
    }
    if (["completed", "cancelled", "failed"].includes(event.kind)) terminal = event;
  }

  if (!permission) {
    throw new Error(
      `prompt reached a terminal state without an ACP permission request: ${JSON.stringify(observedEvents)}`,
    );
  }
  if (permission.category !== "filesystem.write") {
    throw new Error(`expected filesystem.write, received ${permission.category}`);
  }
  try {
    await readFile(target);
    throw new Error(`denied target was created: ${target}`);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  const history = await request("permissions.history", { session_id: session.id });
  const audit = history.records.find(({ request: item }) => item.id === permission.id);
  if (audit?.status !== "denied") throw new Error("permission denial was not retained in audit history");

  console.log(JSON.stringify({
    agent_id: agentID,
    adapter: agent.adapter,
    version: agent.version,
    enforced_session_mode: agent.enforced_session_mode,
    permission_category: permission.category,
    permission_target: permission.target ?? null,
    decision: audit.decision,
    status: audit.status,
    terminal: terminal.kind,
    denied_target_absent: true,
    workspace,
  }, null, 2));
} catch (error) {
  console.error(error.stack ?? String(error));
  if (coreErrors.length > 0) console.error(coreErrors.join("\n"));
  process.exitCode = 1;
} finally {
  await stopCore();
  await rm(root, { recursive: true, force: true });
}
