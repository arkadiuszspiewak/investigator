import fs from "node:fs/promises";
import process from "node:process";

import { getTokenProvider } from "@aws/bedrock-token-generator";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import OpenAI from "openai";

const MAX_TURNS = 32;
const RESULT_PATH = "/dev/termination-log";

function required(name) {
  const value = process.env[name];
  if (!value) throw new Error(`missing required environment variable ${name}`);
  return value;
}

function toolName(server, tool) {
  const normalized = `mcp__${server}__${tool}`.replace(/[^a-zA-Z0-9_-]/g, "_");
  if (normalized.length <= 64) return normalized;
  throw new Error(`MCP tool name exceeds 64 characters: ${normalized}`);
}

async function connectMcpServers() {
  const configured = JSON.parse(process.env.INVESTIGATOR_MCP_SERVERS || "[]");
  if (!Array.isArray(configured)) throw new Error("INVESTIGATOR_MCP_SERVERS must be an array");

  const clients = [];
  const tools = [];
  const routes = new Map();
  for (const server of configured) {
    if (!server?.name || !server?.url) throw new Error("every MCP server requires name and url");
    const client = new Client({ name: "investigator-bedrock-agent", version: "1.0.0" });
    await client.connect(new StreamableHTTPClientTransport(new URL(server.url)));
    clients.push(client);
    const listed = await client.listTools();
    for (const tool of listed.tools) {
      const name = toolName(server.name, tool.name);
      if (routes.has(name)) throw new Error(`duplicate flattened MCP tool name: ${name}`);
      routes.set(name, { client, originalName: tool.name });
      tools.push({
        type: "function",
        name,
        description: tool.description || `Call ${tool.name} on the ${server.name} MCP server`,
        parameters: tool.inputSchema || { type: "object", properties: {} },
        strict: false,
      });
    }
  }
  return { clients, routes, tools };
}

function responseText(response) {
  if (response.output_text) return response.output_text;
  return (response.output || [])
    .flatMap((item) => item.content || [])
    .filter((item) => item.type === "output_text")
    .map((item) => item.text)
    .join("");
}

async function apiKey() {
  if (process.env.BEDROCK_API_KEY) return process.env.BEDROCK_API_KEY;
  required("AWS_REGION");
  return getTokenProvider()();
}

async function createClient() {
  return new OpenAI({
    apiKey: await apiKey(),
    baseURL: required("BEDROCK_BASE_URL"),
    defaultHeaders: { "OpenAI-Project": required("BEDROCK_PROJECT_ID") },
  });
}

async function callMcp(routes, name, argumentsJson) {
  const route = routes.get(name);
  if (!route) throw new Error(`model requested unknown tool ${name}`);
  const result = await route.client.callTool({
    name: route.originalName,
    arguments: JSON.parse(argumentsJson || "{}"),
  });
  return JSON.stringify(result.content ?? result);
}

async function runResponses(model, instructions, prompt, tools, routes) {
  let response = await (await createClient()).responses.create({
    model, instructions, input: prompt, tools, store: true,
  });
  for (let turn = 0; turn < MAX_TURNS; turn += 1) {
    const calls = (response.output || []).filter((item) => item.type === "function_call");
    if (calls.length === 0) return responseText(response);
    const outputs = await Promise.all(calls.map(async (call) => ({
      type: "function_call_output",
      call_id: call.call_id,
      output: await callMcp(routes, call.name, call.arguments),
    })));
    response = await (await createClient()).responses.create({
      model,
      instructions,
      previous_response_id: response.id,
      input: outputs,
      tools,
      store: true,
    });
  }
  throw new Error(`agent exceeded ${MAX_TURNS} model turns`);
}

async function runChatCompletions(model, instructions, prompt, tools, routes) {
  const messages = [
    { role: "system", content: instructions },
    { role: "user", content: prompt },
  ];
  const chatTools = tools.map(({ name, description, parameters }) => ({
    type: "function",
    function: { name, description, parameters },
  }));
  for (let turn = 0; turn < MAX_TURNS; turn += 1) {
    const completion = await (await createClient()).chat.completions.create({
      model,
      messages,
      tools: chatTools,
      tool_choice: "auto",
    });
    const message = completion.choices[0]?.message;
    if (!message) throw new Error("Bedrock returned no chat completion choice");
    const calls = message.tool_calls || [];
    if (calls.length === 0) return message.content || "";
    messages.push({ role: "assistant", content: message.content, tool_calls: calls });
    const outputs = await Promise.all(calls.map(async (call) => ({
      role: "tool",
      tool_call_id: call.id,
      content: await callMcp(routes, call.function.name, call.function.arguments),
    })));
    messages.push(...outputs);
  }
  throw new Error(`agent exceeded ${MAX_TURNS} model turns`);
}

async function run(prompt) {
  const model = required("INVESTIGATOR_AGENT_MODEL");
  const instructions = await fs.readFile("/workspace/AGENTS.md", "utf8");
  const { clients, routes, tools } = await connectMcpServers();
  try {
    switch (required("BEDROCK_API")) {
      case "responses":
        return await runResponses(model, instructions, prompt, tools, routes);
      case "chatCompletions":
        return await runChatCompletions(model, instructions, prompt, tools, routes);
      default:
        throw new Error(`unsupported BEDROCK_API: ${process.env.BEDROCK_API}`);
    }
  } finally {
    await Promise.allSettled(clients.map((client) => client.close()));
  }
}

const prompt = process.argv.slice(2).join(" ").trim();
if (!prompt) throw new Error("missing investigation prompt");

try {
  const result = await run(prompt);
  await fs.writeFile(RESULT_PATH, result || "STATUS: INCONCLUSIVE\n\nSUMMARY:\nThe model returned no final response.");
  process.stdout.write(`${result}\n`);
} catch (error) {
  const message = error instanceof Error ? error.stack || error.message : String(error);
  process.stderr.write(`${message}\n`);
  process.exitCode = 1;
}
