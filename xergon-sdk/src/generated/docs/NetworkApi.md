# NetworkApi

All URIs are relative to *https://relay.xergon.gg*

| Method | HTTP request | Description |
|------------- | ------------- | -------------|
| [**authStatus**](NetworkApi.md#authstatus) | **GET** /v1/auth/status | Verify authentication |
| [**getBalance**](NetworkApi.md#getbalance) | **GET** /v1/balance/{user_pk} | Check user ERG balance |
| [**getLeaderboard**](NetworkApi.md#getleaderboard) | **GET** /v1/leaderboard | Provider leaderboard |
| [**listProviders**](NetworkApi.md#listproviders) | **GET** /v1/providers | List active providers |



## authStatus

> AuthStatus200Response authStatus()

Verify authentication

Check if the provided HMAC signature is valid

### Example

```ts
import {
  Configuration,
  NetworkApi,
} from '';
import type { AuthStatusRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new NetworkApi(config);

  try {
    const data = await api.authStatus();
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

[**AuthStatus200Response**](AuthStatus200Response.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Auth status |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## getBalance

> BalanceResponse getBalance(userPk)

Check user ERG balance

Returns the user\&#39;s available ERG balance from their on-chain Staking Box

### Example

```ts
import {
  Configuration,
  NetworkApi,
} from '';
import type { GetBalanceRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new NetworkApi(config);

  const body = {
    // string | User\'s Ergo public key (hex)
    userPk: userPk_example,
  } satisfies GetBalanceRequest;

  try {
    const data = await api.getBalance(body);
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
| **userPk** | `string` | User\&#39;s Ergo public key (hex) | [Defaults to `undefined`] |

### Return type

[**BalanceResponse**](BalanceResponse.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Balance info |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## getLeaderboard

> Array&lt;ProviderEntry&gt; getLeaderboard(limit, offset)

Provider leaderboard

Providers ranked by PoNW (Proof of Network Work) score

### Example

```ts
import {
  Configuration,
  NetworkApi,
} from '';
import type { GetLeaderboardRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new NetworkApi(config);

  const body = {
    // number (optional)
    limit: 56,
    // number (optional)
    offset: 56,
  } satisfies GetLeaderboardRequest;

  try {
    const data = await api.getLeaderboard(body);
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
| **limit** | `number` |  | [Optional] [Defaults to `20`] |
| **offset** | `number` |  | [Optional] [Defaults to `0`] |

### Return type

[**Array&lt;ProviderEntry&gt;**](ProviderEntry.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Leaderboard data |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## listProviders

> Array&lt;ProviderEntry&gt; listProviders()

List active providers

All providers currently serving inference, read from chain state

### Example

```ts
import {
  Configuration,
  NetworkApi,
} from '';
import type { ListProvidersRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const config = new Configuration({ 
    // To configure API key authorization: hmacSignature
    apiKey: "YOUR API KEY",
  });
  const api = new NetworkApi(config);

  try {
    const data = await api.listProviders();
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

[**Array&lt;ProviderEntry&gt;**](ProviderEntry.md)

### Authorization

[hmacSignature](../README.md#hmacSignature)

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: `application/json`


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Provider list |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)

