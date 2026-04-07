# BridgeApi

All URIs are relative to *https://relay.xergon.gg*

| Method | HTTP request | Description |
|------------- | ------------- | -------------|
| [**bridgeStatus**](BridgeApi.md#bridgestatus) | **GET** /v1/bridge/status | Bridge status |
| [**confirmPayment**](BridgeApi.md#confirmpaymentoperation) | **POST** /v1/bridge/confirm | Confirm payment |
| [**createInvoice**](BridgeApi.md#createinvoiceoperation) | **POST** /v1/bridge/create-invoice | Create payment invoice |
| [**getInvoice**](BridgeApi.md#getinvoice) | **GET** /v1/bridge/invoice/{id} | Get invoice status |
| [**listInvoices**](BridgeApi.md#listinvoices) | **GET** /v1/bridge/invoices | List invoices |
| [**refundInvoice**](BridgeApi.md#refundinvoiceoperation) | **POST** /v1/bridge/refund | Refund invoice |



## bridgeStatus

> BridgeStatus200Response bridgeStatus()

Bridge status

### Example

```ts
import {
  Configuration,
  BridgeApi,
} from '';
import type { BridgeStatusRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new BridgeApi(config);

  try {
    const data = await api.bridgeStatus();
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

[**BridgeStatus200Response**](BridgeStatus200Response.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Bridge operational status |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## confirmPayment

> confirmPayment(confirmPaymentRequest)

Confirm payment

### Example

```ts
import {
  Configuration,
  BridgeApi,
} from '';
import type { ConfirmPaymentOperationRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new BridgeApi(config);

  const body = {
    // ConfirmPaymentRequest
    confirmPaymentRequest: ...,
  } satisfies ConfirmPaymentOperationRequest;

  try {
    const data = await api.confirmPayment(body);
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
| **confirmPaymentRequest** | [ConfirmPaymentRequest](ConfirmPaymentRequest.md) |  | |

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
| **200** | Payment confirmed |  -  |
| **400** | Invalid request |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## createInvoice

> BridgeInvoice createInvoice(createInvoiceRequest)

Create payment invoice

### Example

```ts
import {
  Configuration,
  BridgeApi,
} from '';
import type { CreateInvoiceOperationRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new BridgeApi(config);

  const body = {
    // CreateInvoiceRequest
    createInvoiceRequest: ...,
  } satisfies CreateInvoiceOperationRequest;

  try {
    const data = await api.createInvoice(body);
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
| **createInvoiceRequest** | [CreateInvoiceRequest](CreateInvoiceRequest.md) |  | |

### Return type

[**BridgeInvoice**](BridgeInvoice.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: `application/json`
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Invoice created |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## getInvoice

> BridgeInvoice getInvoice(id)

Get invoice status

### Example

```ts
import {
  Configuration,
  BridgeApi,
} from '';
import type { GetInvoiceRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new BridgeApi(config);

  const body = {
    // string
    id: id_example,
  } satisfies GetInvoiceRequest;

  try {
    const data = await api.getInvoice(body);
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
| **id** | `string` |  | [Defaults to `undefined`] |

### Return type

[**BridgeInvoice**](BridgeInvoice.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Invoice detail |  -  |
| **404** | Resource not found |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## listInvoices

> Array&lt;BridgeInvoice&gt; listInvoices()

List invoices

### Example

```ts
import {
  Configuration,
  BridgeApi,
} from '';
import type { ListInvoicesRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new BridgeApi(config);

  try {
    const data = await api.listInvoices();
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

[**Array&lt;BridgeInvoice&gt;**](BridgeInvoice.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Invoice list |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## refundInvoice

> refundInvoice(refundInvoiceRequest)

Refund invoice

### Example

```ts
import {
  Configuration,
  BridgeApi,
} from '';
import type { RefundInvoiceOperationRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new BridgeApi(config);

  const body = {
    // RefundInvoiceRequest
    refundInvoiceRequest: ...,
  } satisfies RefundInvoiceOperationRequest;

  try {
    const data = await api.refundInvoice(body);
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
| **refundInvoiceRequest** | [RefundInvoiceRequest](RefundInvoiceRequest.md) |  | |

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
| **200** | Refund processed |  -  |
| **400** | Invalid request |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)

