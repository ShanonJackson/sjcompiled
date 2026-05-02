import valueParser from "postcss-value-parser";

function dump(label, input) {
  const parsed = valueParser(input);
  console.log(`--- ${label}: ${JSON.stringify(input)} ---`);
  console.log(JSON.stringify(parsed.nodes, null, 2));
}

dump("A", "1 / 2");
dump("B", "(1) / 2");
dump("C", "url(foo) / 2");
dump("D", "url(  foo.png  )");
try { dump("E", "a\\"); } catch (e) { console.log("E: threw " + e.message); }
