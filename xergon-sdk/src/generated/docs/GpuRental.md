
# GpuRental


## Properties

Name | Type
------------ | -------------
`rentalId` | string
`listingId` | string
`providerPk` | string
`renterPk` | string
`hours` | number
`costNanoerg` | string
`startedAt` | number
`expiresAt` | number
`status` | string

## Example

```typescript
import type { GpuRental } from ''

// TODO: Update the object below with actual values
const example = {
  "rentalId": null,
  "listingId": null,
  "providerPk": null,
  "renterPk": null,
  "hours": null,
  "costNanoerg": null,
  "startedAt": null,
  "expiresAt": null,
  "status": null,
} satisfies GpuRental

console.log(example)

// Convert the instance to a JSON string
const exampleJSON: string = JSON.stringify(example)
console.log(exampleJSON)

// Parse the JSON string back to an object
const exampleParsed = JSON.parse(exampleJSON) as GpuRental
console.log(exampleParsed)
```

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


