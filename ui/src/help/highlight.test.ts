// @vitest-environment happy-dom
import { expect, it } from "vitest";
import { elementFor, locateFeature, onReveal } from "./highlight";
it("directs reveal actions to the active boat, including when its handler registered first", async () => {
  let active=1;const seen:number[]=[];
  const off1=onReveal("boat-test:reveal",()=>{seen.push(1);},()=>active===1);
  const off2=onReveal("boat-test:reveal",()=>{seen.push(2);},()=>active===2);
  const button=document.createElement("button");button.dataset.feature="boat-test:target";
  button.getBoundingClientRect=()=>new DOMRect(0,0,20,20);document.body.append(button);
  try {
    const feature={id:"boat-test:target",label:"test",reveal:["boat-test:reveal"]};
    await locateFeature(feature);active=2;await locateFeature(feature);
    expect(seen).toEqual([1,2]);
  } finally {off1();off2();button.remove();window.dispatchEvent(new Event("pointerdown"));}
});
it("finds the active pane's control before another visible pane and skips hidden tabs",()=>{
  const host=document.createElement("div");
  host.innerHTML='<section class="boat-pane"><button data-feature="boat-test:point"></button></section><section class="boat-pane active"><button data-feature="boat-test:point"></button></section>';
  document.body.append(host);
  const buttons=[...host.querySelectorAll<HTMLButtonElement>("button")];for(const button of buttons)button.getBoundingClientRect=()=>new DOMRect(0,0,20,20);
  expect(elementFor("boat-test:point")).toBe(buttons[1]);
  host.children[1]!.setAttribute("hidden","");expect(elementFor("boat-test:point")).toBe(buttons[0]);host.remove();
});
