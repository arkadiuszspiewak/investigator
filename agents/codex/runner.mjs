import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import readline from "node:readline";

function logMeasurement(event, fields = {}) {
  process.stderr.write(`${JSON.stringify({ event, ...fields })}\n`);
}

function recordMcpResponse(event) {
  if (event.type !== "item.completed" || event.item?.type !== "mcp_tool_call") return;

  const result = event.item.result;
  logMeasurement("investigator_mcp_tool_response", {
    server: event.item.server,
    tool: event.item.tool,
    response_bytes: result === undefined ? 0 : Buffer.byteLength(JSON.stringify(result), "utf8"),
    success: event.item.status === "completed",
    duration_ms: event.item.duration_ms,
    measurement_source: "codex_exec_jsonl",
  });
}

async function sessionFiles(directory) {
  const files = [];
  let entries;
  try {
    entries = await fs.readdir(directory, { withFileTypes: true });
  } catch (error) {
    if (error?.code === "ENOENT") return files;
    throw error;
  }
  for (const entry of entries) {
    const child = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...await sessionFiles(child));
    else if (entry.isFile() && entry.name.endsWith(".jsonl")) files.push(child);
  }
  return files;
}

async function recordInferenceUsage(codexHome, startedAt) {
  const files = await sessionFiles(path.join(codexHome, "sessions"));
  let inference = 0;
  for (const file of files.sort()) {
    const stat = await fs.stat(file);
    if (stat.mtimeMs < startedAt) continue;
    const lines = (await fs.readFile(file, "utf8")).split("\n");
    for (const line of lines) {
      if (!line) continue;
      let event;
      try {
        event = JSON.parse(line);
      } catch {
        continue;
      }
      const usage = event.type === "event_msg" && event.payload?.type === "token_count"
        ? event.payload.info?.last_token_usage
        : undefined;
      if (!usage) continue;
      inference += 1;
      logMeasurement("investigator_llm_inference_usage", {
        inference,
        input_tokens: usage.input_tokens ?? 0,
        cached_input_tokens: usage.cached_input_tokens ?? 0,
        cache_write_input_tokens: usage.cache_write_input_tokens ?? 0,
        output_tokens: usage.output_tokens ?? 0,
        reasoning_output_tokens: usage.reasoning_output_tokens ?? 0,
        total_tokens: usage.total_tokens
          ?? (usage.input_tokens ?? 0) + (usage.output_tokens ?? 0),
        measurement_source: "codex_session_last_token_usage",
      });
    }
  }
  if (inference === 0) {
    logMeasurement("investigator_llm_inference_usage_unavailable", {
      measurement_source: "codex_session_last_token_usage",
    });
  }
}

const args = process.argv.slice(2);
if (args[0] !== "exec") throw new Error("Codex telemetry runner only supports `codex exec`");
args.splice(1, 0, "--json");

const startedAt = Date.now();
const codex = spawn("codex", args, { stdio: ["inherit", "pipe", "inherit"] });
const lines = readline.createInterface({ input: codex.stdout });
lines.on("line", (line) => {
  try {
    recordMcpResponse(JSON.parse(line));
  } catch {
    process.stdout.write(`${line}\n`);
  }
});

const exitCode = await new Promise((resolve, reject) => {
  codex.once("error", reject);
  codex.once("close", (code, signal) => {
    if (signal) reject(new Error(`codex terminated by signal ${signal}`));
    else resolve(code ?? 1);
  });
});
await recordInferenceUsage(process.env.CODEX_HOME || path.join(process.env.HOME, ".codex"), startedAt);
process.exitCode = exitCode;
