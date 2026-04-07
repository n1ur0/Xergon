
# ChatCompletionResponseUsage


## Properties

Name | Type
------------ | -------------
`promptTokens` | number
`completionTokens` | number
`totalTokens` | number

## Example

```typescript
import type { ChatCompletionResponseUsage } from ''

// TODO: Update the object below with actual values
const example = {
  "promptTokens": null,
  "completionTokens": null,
  "totalTokens": null,
} satisfies ChatCompletionResponseUsage

console.log(example)

// Convert the instance to a JSON string
const exampleJSON: string = JSON.stringify(example)
console.log(exampleJSON)

// Parse the JSON string back to an object
const exampleParsed = JSON.parse(exampleJSON) as ChatCompletionResponseUsage
console.log(exampleParsed)
```

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


