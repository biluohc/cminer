#! /usr/bin/env sh
/*
exec deno run --unstable -A --no-check "$0" "$@"
*/

import { deadline, delay } from "https://deno.land/std@0.105.0/async/mod.ts";
import { Client, Pool } from "https://deno.land/x/postgres@v0.13.0/mod.ts";

const pgu =
  "postgres://template:MTcwNzUyNzIzMDY4Nzk2MzQ3Mjg=@0.0.0.0:5433/templatedb";

const size = +Deno.args[0] || 1;

const connections = [];
for (const [idx, _] of new Array(size).entries()) {
  const client = new Client(pgu);
  await deadline(client.connect(), 1000 * 5);

  {
    const result = await client.queryObject(
      "SELECT current_timestamp as ts",
    );
    console.info(idx, result.rows);
  }
  connections.push(client);
}

await delay(1000 * 100);

connections.forEach((c) => {
  c.end();
});
