
# BridgeInvoice


## Properties

Name | Type
------------ | -------------
`invoiceId` | string
`amountNanoerg` | string
`chain` | string
`status` | string
`createdAt` | number
`refundTimeout` | number

## Example

```typescript
import type { BridgeInvoice } from ''

// TODO: Update the object below with actual values
const example = {
  "invoiceId": null,
  "amountNanoerg": null,
  "chain": null,
  "status": null,
  "createdAt": null,
  "refundTimeout": null,
} satisfies BridgeInvoice

console.log(example)

// Convert the instance to a JSON string
const exampleJSON: string = JSON.stringify(example)
console.log(exampleJSON)

// Parse the JSON string back to an object
const exampleParsed = JSON.parse(exampleJSON) as BridgeInvoice
console.log(exampleParsed)
```

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


