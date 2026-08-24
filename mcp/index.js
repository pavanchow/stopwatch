#!/usr/bin/env node
// Stopwatch MCP server. Lets an agent run a built-in workload under the
// profiler and read back the call tree, the epitaph, folded flamegraph
// stacks, or the JSON tree. It shells out to the `stopwatch` binary.
//
// A deliberate scope note: this server does NOT compile or run arbitrary
// code. Profiling an agent's own Rust is done by embedding the crate (a
// few lines, see mcp/README.md), which keeps the profiled program in the
// caller's own trust boundary rather than turning this into a remote code
// execution service.

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { execFile } from "node:child_process";

const BIN = process.env.STOPWATCH_BIN || "stopwatch";
const WORKLOADS = ["fib", "sort", "blur", "mixed"];
const FORMATS = { report: [], json: ["--json"], flamegraph: ["--flamegraph"] };

function run(args) {
  return new Promise((resolve, reject) => {
    execFile(BIN, args, { timeout: 20000, maxBuffer: 32 * 1024 * 1024 }, (err, stdout, stderr) => {
      if (err) reject(new Error((stderr || err.message || "run failed").trim()));
      else resolve(stdout);
    });
  });
}

const server = new Server(
  { name: "stopwatch", version: "0.1.0" },
  { capabilities: { tools: {} } }
);

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: [
    {
      name: "run_workload",
      description:
        "Run a built-in Stopwatch workload (fib, sort, blur, or mixed) under the profiler and return the result. format 'report' gives the call tree plus the epitaph, 'json' gives the raw tree, 'flamegraph' gives folded stacks for flamegraph.pl. Use this to see how self time and total time separate across a call tree.",
      inputSchema: {
        type: "object",
        properties: {
          workload: { type: "string", enum: WORKLOADS, description: "which workload to profile" },
          format: { type: "string", enum: Object.keys(FORMATS), description: "report, json, or flamegraph (default report)" },
        },
        required: ["workload"],
      },
    },
  ],
}));

server.setRequestHandler(CallToolRequestSchema, async (req) => {
  if (req.params.name !== "run_workload") {
    return { isError: true, content: [{ type: "text", text: `unknown tool: ${req.params.name}` }] };
  }
  const a = req.params.arguments || {};
  if (!WORKLOADS.includes(a.workload)) {
    return { isError: true, content: [{ type: "text", text: "workload must be one of " + WORKLOADS.join(", ") }] };
  }
  const fmt = a.format && FORMATS[a.format] ? a.format : "report";
  try {
    const out = await run(["run", "--workload", a.workload, ...FORMATS[fmt]]);
    return { content: [{ type: "text", text: out.trim() }] };
  } catch (e) {
    return { isError: true, content: [{ type: "text", text: e.message }] };
  }
});

const transport = new StdioServerTransport();
await server.connect(transport);
