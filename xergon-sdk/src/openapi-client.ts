/**
 * OpenAPI client -- fetches and introspects the relay's OpenAPI spec.
 *
 * Allows consumers to discover available endpoints and request/response
 * schemas at runtime.
 */

import type { OpenAPISpec, ApiEndpoint, JSONSchema } from './openapi-types';

export class OpenAPIClient {
  private baseUrl: string;
  private cachedSpec: OpenAPISpec | null = null;

  constructor(baseURL: string) {
    this.baseUrl = baseURL.replace(/\/+$/, '');
  }

  /**
   * Fetch the OpenAPI spec from the relay (GET /v1/openapi.json).
   * Results are cached after the first successful fetch.
   */
  async getSpec(): Promise<OpenAPISpec> {
    if (this.cachedSpec) {
      return this.cachedSpec;
    }

    const url = `${this.baseUrl}/v1/openapi.json`;
    const res = await fetch(url, {
      headers: { Accept: 'application/json' },
    });

    if (!res.ok) {
      throw new Error(
        `Failed to fetch OpenAPI spec: ${res.status} ${res.statusText}`,
      );
    }

    this.cachedSpec = (await res.json()) as OpenAPISpec;
    return this.cachedSpec;
  }

  /**
   * Clear the cached spec, forcing a fresh fetch on next getSpec() call.
   */
  clearCache(): void {
    this.cachedSpec = null;
  }

  /**
   * Get all available endpoints from the spec.
   * Returns a flat list of { method, path, ... } objects.
   */
  getEndpoints(): ApiEndpoint[] {
    if (!this.cachedSpec) {
      throw new Error('Spec not loaded. Call getSpec() first.');
    }

    const endpoints: ApiEndpoint[] = [];

    for (const [path, methods] of Object.entries(this.cachedSpec.paths)) {
      for (const [method, operation] of Object.entries(methods)) {
        if (['get', 'post', 'put', 'delete', 'patch'].includes(method)) {
          const op = operation as ApiEndpoint;
          endpoints.push({
            method: method.toUpperCase(),
            path,
            operationId: op.operationId,
            summary: op.summary,
            description: op.description,
            tags: op.tags,
            requestBody: op.requestBody,
            responses: op.responses,
          });
        }
      }
    }

    return endpoints;
  }

  /**
   * Get a schema by name from the components/schemas section.
   */
  getSchema(name: string): JSONSchema | undefined {
    if (!this.cachedSpec?.components?.schemas) {
      return undefined;
    }
    return this.cachedSpec.components.schemas[name] as JSONSchema | undefined;
  }

  /**
   * Find an endpoint by its operationId.
   */
  getEndpointById(operationId: string): ApiEndpoint | undefined {
    return this.getEndpoints().find((ep) => ep.operationId === operationId);
  }

  /**
   * Get all endpoints for a given tag.
   */
  getEndpointsByTag(tag: string): ApiEndpoint[] {
    return this.getEndpoints().filter(
      (ep) => ep.tags && ep.tags.includes(tag),
    );
  }
}
