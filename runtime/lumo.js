"use strict";
// Рантайм Lumo. Только стандартный JS и console — без API Node.

function $idiv(a, b) {
  return b === 0 ? 0 : Math.trunc(a / b);
}

function $imod(a, b) {
  return b === 0 ? 0 : a % b;
}

function $eq(a, b) {
  if (a === b) return true;
  if (typeof a === "function" || typeof b === "function") {
    throw new Error("Lumo: функции нельзя сравнивать через ==");
  }
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) {
    return false;
  }
  const ka = Object.keys(a);
  const kb = Object.keys(b);
  if (ka.length !== kb.length) return false;
  for (const k of ka) {
    if (!$eq(a[k], b[k])) return false;
  }
  return true;
}

function $print(s) {
  console.log(s);
}
