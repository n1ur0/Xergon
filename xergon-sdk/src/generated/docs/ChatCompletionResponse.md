
# ChatCompletionResponse


## Properties

Name | Type
------------ | -------------
`id` | string
`object` | string
`created` | number
`model` | string
`choices` | [Array&lt;ChatCompletionResponseChoicesInner&gt;](ChatCompletionResponseChoicesInner.md)
`usage` | [ChatCompletionResponseUsage](ChatCompletionResponseUsage.md)

## Example

```typescript
import type { ChatCompletionResponse } from ''

// TODO: Update the object below with actual values
const example = {
  "id": null,
  "object": chat.completion,
  "created": null,
  "model": null,
  "choices": null,
  "usage": null,
} satisfies ChatCompletionResponse

console.log(example)

// Convert the instance to a JSON string
const exampleJSON: string = JSON.stringify(example)
console.log(exampleJSON)

// Parse the JSON string back to an object
const exampleParsed = JSON.parse(exampleJSON) as ChatCompletionResponse
console.log(exampleParsed)
```

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


