
# ProviderEntry


## Properties

Name | Type
------------ | -------------
`publicKey` | string
`endpoint` | string
`models` | Array&lt;string&gt;
`region` | string
`pownScore` | number
`lastHeartbeat` | number
`pricing` | { [key: string]: string; }

## Example

```typescript
import type { ProviderEntry } from ''

// TODO: Update the object below with actual values
const example = {
  "publicKey": null,
  "endpoint": null,
  "models": null,
  "region": null,
  "pownScore": null,
  "lastHeartbeat": null,
  "pricing": null,
} satisfies ProviderEntry

console.log(example)

// Convert the instance to a JSON string
const exampleJSON: string = JSON.stringify(example)
console.log(exampleJSON)

// Parse the JSON string back to an object
const exampleParsed = JSON.parse(exampleJSON) as ProviderEntry
console.log(exampleParsed)
```

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


