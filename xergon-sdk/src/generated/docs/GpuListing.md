
# GpuListing


## Properties

Name | Type
------------ | -------------
`listingId` | string
`providerPk` | string
`gpuType` | string
`vramGb` | number
`pricePerHourNanoerg` | string
`region` | string
`available` | boolean
`bandwidthMbps` | number

## Example

```typescript
import type { GpuListing } from ''

// TODO: Update the object below with actual values
const example = {
  "listingId": null,
  "providerPk": null,
  "gpuType": null,
  "vramGb": null,
  "pricePerHourNanoerg": null,
  "region": null,
  "available": null,
  "bandwidthMbps": null,
} satisfies GpuListing

console.log(example)

// Convert the instance to a JSON string
const exampleJSON: string = JSON.stringify(example)
console.log(exampleJSON)

// Parse the JSON string back to an object
const exampleParsed = JSON.parse(exampleJSON) as GpuListing
console.log(exampleParsed)
```

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


