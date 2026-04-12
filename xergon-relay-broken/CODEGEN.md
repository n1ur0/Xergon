# OpenAPI Code Generation

The Xergon Relay exposes an OpenAPI 3.0.2 spec at `docs/openapi.yaml`. Client SDKs
(TypeScript and Python) can be auto-generated from this single source of truth.

## Prerequisites

- **Node.js >= 18** — the codegen script uses `npx` to fetch
  [openapi-generator-cli](https://github.com/OpenAPITools/openapi-generator-cli)
  automatically. If you prefer, install it globally:

  ```bash
  npm install -g @openapitools/openapi-generator-cli
  ```

- **Repository layout** — the SDK repos must be siblings of `xergon-relay`:

  ```
  Xergon-Network/
  ├── xergon-relay/          # this repo
  ├── xergon-sdk/            # TypeScript SDK
  └── xergon-sdk-python/     # Python SDK
  ```

## Quick Start

From the `xergon-relay/` directory:

```bash
# Generate both TypeScript and Python clients
make codegen

# Or run the script directly:
./scripts/codegen.sh

# Generate only one target:
./scripts/codegen.sh typescript
./scripts/codegen.sh python
```

## Output Locations

| Target    | Output directory                                          |
|-----------|-----------------------------------------------------------|
| TypeScript | `../xergon-sdk/src/generated/`                            |
| Python     | `../xergon-sdk-python/src/xergon/generated/`              |

### TypeScript Output

The `typescript-fetch` generator produces:

- `apis/` — API client classes (InferenceApi, NetworkApi, GPUBazarApi, etc.)
- `models/` — TypeScript interfaces for all request/response schemas
- `runtime.ts` — Base HTTP configuration and fetch utilities
- `index.ts` — Barrel export

The generated code uses ES6+ and TypeScript 3.x+ compatible output.

### Python Output

The `python` generator produces a `xergon.generated` package with:

- `api/` — API client classes
- `models/` — Model classes (dataclasses-based)
- `api_client.py` — HTTP client
- `configuration.py` — Client configuration
- `exceptions.py` — Custom exceptions
- `rest.py` — REST transport layer

Only the package contents (`xergon/generated/`) are copied into the SDK; the
generator's scaffolding (setup.py, tests/, etc.) is discarded.

## Integrating Generated Code

### TypeScript SDK

The generated types complement (but don't replace) the hand-written types in
`src/types.ts`. You can re-export generated models:

```typescript
// src/generated/index.ts is auto-generated and re-exports everything.
// Import from the SDK's own types for the idiomatic camelCase wrappers.
// Use generated types directly when you need the exact wire format.

import { ChatCompletionRequest } from './generated';
import { ChatCompletionResponse } from './generated/models/ChatCompletionResponse';
```

### Python SDK

The generated package sits alongside the hand-written `xergon.types` module:

```python
from xergon.generated import Configuration, ApiClient
from xergon.generated.api.inference_api import InferenceApi
from xergon.generated.models.chat_completion_request import ChatCompletionRequest
```

## Workflow

1. Edit `docs/openapi.yaml` in `xergon-relay/`
2. Run `make codegen` (or `./scripts/codegen.sh`)
3. Review generated diffs in the SDK repos
4. Commit the updated spec and generated code together

## Notes

- **Inline schemas** — Several response bodies are defined inline in the spec
  rather than as named `$ref` components. The generator auto-names these (e.g.
  `AuthStatus200Response`). To get cleaner names, add a `title` field to those
  schemas in `openapi.yaml`.
- **Spec validation** — The generator validates the spec before generating.
  If you hit validation errors, fix them in `docs/openapi.yaml`.
- **Customizing generation** — Edit `scripts/codegen.sh` to change generators,
  add `--additional-properties`, or adjust model-name mappings.
