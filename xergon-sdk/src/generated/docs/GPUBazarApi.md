# GPUBazarApi

All URIs are relative to *https://relay.xergon.gg*

| Method | HTTP request | Description |
|------------- | ------------- | -------------|
| [**getGpuListing**](GPUBazarApi.md#getgpulisting) | **GET** /v1/gpu/listings/{listing_id} | Get GPU listing details |
| [**getGpuPricing**](GPUBazarApi.md#getgpupricing) | **GET** /v1/gpu/pricing | GPU pricing information |
| [**getGpuRentals**](GPUBazarApi.md#getgpurentals) | **GET** /v1/gpu/rentals/{renter_pk} | Get user\&#39;s active rentals |
| [**getGpuReputation**](GPUBazarApi.md#getgpureputation) | **GET** /v1/gpu/reputation/{public_key} | Get reputation score |
| [**listGpuListings**](GPUBazarApi.md#listgpulistings) | **GET** /v1/gpu/listings | Browse GPU listings |
| [**rateGpu**](GPUBazarApi.md#rategpuoperation) | **POST** /v1/gpu/rate | Rate a GPU provider or renter |
| [**rentGpu**](GPUBazarApi.md#rentgpuoperation) | **POST** /v1/gpu/rent | Rent a GPU |



## getGpuListing

> GpuListing getGpuListing(listingId)

Get GPU listing details

### Example

```ts
import {
  Configuration,
  GPUBazarApi,
} from '';
import type { GetGpuListingRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new GPUBazarApi(config);

  const body = {
    // string
    listingId: listingId_example,
  } satisfies GetGpuListingRequest;

  try {
    const data = await api.getGpuListing(body);
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
| **listingId** | `string` |  | [Defaults to `undefined`] |

### Return type

[**GpuListing**](GpuListing.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Listing details |  -  |
| **404** | Resource not found |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## getGpuPricing

> GetGpuPricing200Response getGpuPricing()

GPU pricing information

### Example

```ts
import {
  Configuration,
  GPUBazarApi,
} from '';
import type { GetGpuPricingRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new GPUBazarApi(config);

  try {
    const data = await api.getGpuPricing();
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

[**GetGpuPricing200Response**](GetGpuPricing200Response.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Pricing data |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## getGpuRentals

> Array&lt;GpuRental&gt; getGpuRentals(renterPk)

Get user\&#39;s active rentals

### Example

```ts
import {
  Configuration,
  GPUBazarApi,
} from '';
import type { GetGpuRentalsRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new GPUBazarApi(config);

  const body = {
    // string
    renterPk: renterPk_example,
  } satisfies GetGpuRentalsRequest;

  try {
    const data = await api.getGpuRentals(body);
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
| **renterPk** | `string` |  | [Defaults to `undefined`] |

### Return type

[**Array&lt;GpuRental&gt;**](GpuRental.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Active rentals |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## getGpuReputation

> GetGpuReputation200Response getGpuReputation(publicKey)

Get reputation score

### Example

```ts
import {
  Configuration,
  GPUBazarApi,
} from '';
import type { GetGpuReputationRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new GPUBazarApi(config);

  const body = {
    // string
    publicKey: publicKey_example,
  } satisfies GetGpuReputationRequest;

  try {
    const data = await api.getGpuReputation(body);
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
| **publicKey** | `string` |  | [Defaults to `undefined`] |

### Return type

[**GetGpuReputation200Response**](GetGpuReputation200Response.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Reputation data |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## listGpuListings

> Array&lt;GpuListing&gt; listGpuListings(gpuType, minVram, maxPrice, region)

Browse GPU listings

### Example

```ts
import {
  Configuration,
  GPUBazarApi,
} from '';
import type { ListGpuListingsRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new GPUBazarApi(config);

  const body = {
    // string (optional)
    gpuType: gpuType_example,
    // number (optional)
    minVram: 56,
    // number (optional)
    maxPrice: 8.14,
    // string (optional)
    region: region_example,
  } satisfies ListGpuListingsRequest;

  try {
    const data = await api.listGpuListings(body);
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
| **gpuType** | `string` |  | [Optional] [Defaults to `undefined`] |
| **minVram** | `number` |  | [Optional] [Defaults to `undefined`] |
| **maxPrice** | `number` |  | [Optional] [Defaults to `undefined`] |
| **region** | `string` |  | [Optional] [Defaults to `undefined`] |

### Return type

[**Array&lt;GpuListing&gt;**](GpuListing.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | GPU listings |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## rateGpu

> rateGpu(rateGpuRequest)

Rate a GPU provider or renter

### Example

```ts
import {
  Configuration,
  GPUBazarApi,
} from '';
import type { RateGpuOperationRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new GPUBazarApi(config);

  const body = {
    // RateGpuRequest
    rateGpuRequest: ...,
  } satisfies RateGpuOperationRequest;

  try {
    const data = await api.rateGpu(body);
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
| **rateGpuRequest** | [RateGpuRequest](RateGpuRequest.md) |  | |

### Return type

`void` (Empty response body)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: `application/json`
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Rating submitted |  -  |
| **400** | Invalid request |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## rentGpu

> GpuRental rentGpu(rentGpuRequest)

Rent a GPU

### Example

```ts
import {
  Configuration,
  GPUBazarApi,
} from '';
import type { RentGpuOperationRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new GPUBazarApi(config);

  const body = {
    // RentGpuRequest
    rentGpuRequest: ...,
  } satisfies RentGpuOperationRequest;

  try {
    const data = await api.rentGpu(body);
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
| **rentGpuRequest** | [RentGpuRequest](RentGpuRequest.md) |  | |

### Return type

[**GpuRental**](GpuRental.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: `application/json`
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Rental created |  -  |
| **400** | Invalid request |  -  |
| **409** | Resource conflict |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)

