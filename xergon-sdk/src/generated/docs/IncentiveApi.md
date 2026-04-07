# IncentiveApi

All URIs are relative to *https://relay.xergon.gg*

| Method | HTTP request | Description |
|------------- | ------------- | -------------|
| [**incentiveModelDetail**](IncentiveApi.md#incentivemodeldetail) | **GET** /v1/incentive/models/{model} | Model rarity detail |
| [**incentiveModels**](IncentiveApi.md#incentivemodels) | **GET** /v1/incentive/models | Rare model bonuses |
| [**incentiveStatus**](IncentiveApi.md#incentivestatus) | **GET** /v1/incentive/status | Incentive system status |



## incentiveModelDetail

> incentiveModelDetail(model)

Model rarity detail

### Example

```ts
import {
  Configuration,
  IncentiveApi,
} from '';
import type { IncentiveModelDetailRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new IncentiveApi(config);

  const body = {
    // string
    model: model_example,
  } satisfies IncentiveModelDetailRequest;

  try {
    const data = await api.incentiveModelDetail(body);
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
| **model** | `string` |  | [Defaults to `undefined`] |

### Return type

`void` (Empty response body)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Model detail |  -  |
| **404** | Resource not found |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## incentiveModels

> Array&lt;IncentiveModels200ResponseInner&gt; incentiveModels()

Rare model bonuses

### Example

```ts
import {
  Configuration,
  IncentiveApi,
} from '';
import type { IncentiveModelsRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new IncentiveApi(config);

  try {
    const data = await api.incentiveModels();
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

[**Array&lt;IncentiveModels200ResponseInner&gt;**](IncentiveModels200ResponseInner.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Models with rarity bonuses |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## incentiveStatus

> IncentiveStatus200Response incentiveStatus()

Incentive system status

### Example

```ts
import {
  Configuration,
  IncentiveApi,
} from '';
import type { IncentiveStatusRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new IncentiveApi(config);

  try {
    const data = await api.incentiveStatus();
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

[**IncentiveStatus200Response**](IncentiveStatus200Response.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | System-wide incentive info |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)

