# InferenceApi

All URIs are relative to *https://relay.xergon.gg*

| Method | HTTP request | Description |
|------------- | ------------- | -------------|
| [**createChatCompletion**](InferenceApi.md#createchatcompletion) | **POST** /v1/chat/completions | Create chat completion |
| [**listModels**](InferenceApi.md#listmodels) | **GET** /v1/models | List available models |



## createChatCompletion

> ChatCompletionResponse createChatCompletion(chatCompletionRequest)

Create chat completion

OpenAI-compatible chat completion endpoint. Supports streaming (SSE) and non-streaming responses. Routes to the best available provider based on PoNW score, latency, and model availability. 

### Example

```ts
import {
  Configuration,
  InferenceApi,
} from '';
import type { CreateChatCompletionRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new InferenceApi(config);

  const body = {
    // ChatCompletionRequest
    chatCompletionRequest: ...,
  } satisfies CreateChatCompletionRequest;

  try {
    const data = await api.createChatCompletion(body);
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters


| Name | Type | Description  | Notes |
|------------- | ------------- | ------------- | -------------|
| **chatCompletionRequest** | [ChatCompletionRequest](ChatCompletionRequest.md) |  | |

### Return type

[**ChatCompletionResponse**](ChatCompletionResponse.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: `application/json`
- **Accept**: `application/json`, `text/event-stream`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Successful completion |  -  |
| **400** | Invalid request |  -  |
| **401** | Invalid or missing authentication |  -  |
| **429** | Rate limit exceeded |  * X-RateLimit-Limit -  <br>  * X-RateLimit-Remaining -  <br>  * X-RateLimit-Reset -  <br>  * Retry-After -  <br>  |
| **503** | No providers available |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## listModels

> ModelsResponse listModels()

List available models

Returns all models currently available from active providers

### Example

```ts
import {
  Configuration,
  InferenceApi,
} from '';
import type { ListModelsRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new InferenceApi(config);

  try {
    const data = await api.listModels();
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters

This endpoint does not need any parameter.

### Return type

[**ModelsResponse**](ModelsResponse.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Model list |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)

