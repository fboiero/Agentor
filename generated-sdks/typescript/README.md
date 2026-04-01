# argentor_client

TypeScript SDK client for the Argentor API.

## Installation

```bash
npm install argentor_client
```

## Quick Start

```typescript
import { ArgentorClient } from 'argentor_client';

const client = new ArgentorClient({
  baseUrl: 'http://localhost:3000',
  apiKey: 'your-api-key',
  tenantId: 'your-tenant-id',
});

// Run a task
const result = await client.runTask(
  'code_reviewer',
  'Review the following pull request...',
);
console.log(result);

// Stream results
for await (const chunk of client.runTaskStream(
  'assistant',
  'Explain how Argentor works',
)) {
  process.stdout.write(JSON.stringify(chunk));
}

// Batch execution
const batchResult = await client.batch([
  { agentRole: 'analyst', context: 'Analyze Q1 sales data' },
  { agentRole: 'analyst', context: 'Analyze Q2 sales data' },
]);

// Evaluate a response
const evaluation = await client.evaluate(
  'The code looks clean and follows best practices.',
  'Code review task',
  ['accuracy', 'completeness', 'helpfulness'],
);

// Health check
const health = await client.health();
console.log(health);
```

## API Reference

### ArgentorClient

| Method | Description |
|--------|-------------|
| `runTask(agentRole, context, options?)` | Execute a single agent task |
| `runTaskStream(agentRole, context, options?)` | Stream task results via SSE |
| `batch(tasks, options?)` | Submit a batch of tasks |
| `evaluate(response, context, criteria?)` | Evaluate an agent response |
| `createPersona(tenantId, agentRole, persona)` | Create a new persona |
| `listPersonas(tenantId)` | List personas for a tenant |
| `getUsage(tenantId)` | Get usage breakdown |
| `health()` | Check API health |
| `webhookProxy(event, data, options?)` | Forward a webhook event |
