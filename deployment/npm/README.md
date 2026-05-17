# @dprint/jupyter

npm distribution of [dprint-plugin-jupyter](https://github.com/dprint/dprint-plugin-jupyter).

Use this with [@dprint/formatter](https://github.com/dprint/js-formatter) or just use @dprint/formatter and download the [dprint-plugin-jupyter WASM file](https://github.com/dprint/dprint-plugin-jupyter/releases).

## Example

```ts
import { createFromBuffer } from "@dprint/formatter";
import { getPath } from "@dprint/jupyter";
import * as fs from "fs";

const buffer = fs.readFileSync(getPath());
const formatter = createFromBuffer(buffer);

console.log(formatter.formatText("notebook.ipynb", "{}"));
```
