"use strict";
// Рантайм Lumo. Только стандартный JS и console — без API Node.

function $idiv(a, b) {
  return b === 0 ? 0 : Math.trunc(a / b);
}

function $imod(a, b) {
  return b === 0 ? 0 : a % b;
}

function $eq(a, b) {
  for (;;) {
    // функции сравниваются с ошибкой всегда, даже `f == f`
    if (typeof a === "function" || typeof b === "function") {
      throw new Error("Lumo: функции нельзя сравнивать через ==");
    }
    if (a === b) return true;
    if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) {
      return false;
    }
    const ka = Object.keys(a);
    const kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (const k of ka) {
      if (k !== "t" && !$eq(a[k], b[k])) return false;
    }
    // ячейка списка: хвост сравниваем циклом, чтобы длинные списки не переполняли стек
    if (!("t" in a)) return true;
    a = a.t;
    b = b.t;
  }
}

function $cons(h, t) {
  return { h, t };
}

// Список из массива элементов и хвоста; цикл вместо вложенных вызовов — длинные литералы не переполняют стек.
function $list(items, tail) {
  let out = tail;
  for (let i = items.length - 1; i >= 0; i--) out = $cons(items[i], out);
  return out;
}

// Значение в синтаксисе Lumo. Целые и дробные числа неразличимы в рантайме: 2.0 печатается как 2.
function $inspect(v) {
  if (v === undefined) return "Unit";
  if (v === null) return "[]";
  if (typeof v === "string") return JSON.stringify(v);
  if (typeof v === "number" || typeof v === "boolean") return String(v);
  if (typeof v === "function") return "<fn>";
  if (Array.isArray(v)) return "#(" + v.map($inspect).join(", ") + ")";
  if ("h" in v) {
    const items = [];
    for (let c = v; c !== null; c = c.t) items.push($inspect(c.h));
    return "[" + items.join(", ") + "]";
  }
  const args = [];
  for (let i = 0; "_" + i in v; i++) args.push($inspect(v["_" + i]));
  return args.length === 0 ? v.$ : v.$ + "(" + args.join(", ") + ")";
}

function $debug(v) {
  console.log($inspect(v));
  return v;
}

function $print(s) {
  console.log(s);
}
