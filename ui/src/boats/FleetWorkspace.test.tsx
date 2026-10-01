// @vitest-environment happy-dom
import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { TEST_BLEND } from "../testBlend";
import type { ProjectSummary } from "../generated/ProjectSummary";
const invoke=vi.hoisted(()=>vi.fn());
const reportFailure=vi.hoisted(()=>vi.fn());
vi.mock("@tauri-apps/api/core",()=>({invoke}));
vi.mock("../errors",()=>({reportFailure}));
vi.mock("./BoatWorkspace",()=>({default:({project,split,visible}: {project:ProjectSummary;split:boolean;visible:boolean})=><div data-testid="workspace" data-mode={split?"3d":"all"} data-visible={visible}>{project.boat_name}</div>}));
const {default:FleetWorkspace}=await import("./FleetWorkspace");
(globalThis as {IS_REACT_ACT_ENVIRONMENT?:boolean}).IS_REACT_ACT_ENVIRONMENT=true;
let host:HTMLDivElement;let toolbar:HTMLDivElement;let root:Root;let docs:ProjectSummary[];let active:ReturnType<typeof vi.fn>;
let deleted: { doc: ProjectSummary; index: number }[];
function doc(id:number,name:string):ProjectSummary{return {id,name:"Fleet",boat_name:name,boat_notes:"",revision:1,path:null,dirty:true,sources:[],can_undo:false,can_redo:false,undo_label:null,redo_label:null,use_corrected:true,stokes_drift:false,blend:TEST_BLEND};}
/** The shell handing down a newer root summary, as it does on `document://changed`. */
let setRoot:(project:ProjectSummary)=>void;
function FleetHarness() {
 const [project,setProject]=useState(docs[0]!);
 setRoot=setProject;
 return <FleetWorkspace project={project} settings={null} onSettings={()=>{}} onProject={setProject} changedBoat={null} onActive={active} toolbarHost={toolbar} onReplace={(_,next)=>setProject(next)}/>;
}
async function click(selector:string){await act(async()=>{(host.querySelector<HTMLButtonElement>(selector) ?? toolbar.querySelector<HTMLButtonElement>(selector))!.click();});}
beforeEach(async()=>{
 docs=[doc(101,"Alpha")];deleted=[];active=vi.fn();invoke.mockReset();reportFailure.mockReset();
 invoke.mockImplementation(async(command:string,args?:Record<string,unknown>)=>{
  if(command==="boat_tabs")return {project_id:docs[0]!.id,name:"Fleet",dirty:true,can_restore:deleted.length>0,tabs:docs.map(d=>({id:d.id,name:d.boat_name,details:{}}))};
  if(command==="project_summary"){
   const found=docs.find(d=>d.id===(args?.boatContext??docs[0]!.id));
   // As the application answers for a boat the project does not hold.
   if(!found)throw {kind:"no-project",message:"No project is open."};
   return found;
  }
  if(command==="add_boat"){const next=doc(101+docs.length,args!.name as string);docs.push(next);return next;}
  if(command==="rename_boat"){const d=docs.find(d=>d.id===args!.boatContext)!; const next={...d,boat_name:args!.name as string,revision:d.revision+1};docs=docs.map(d=>d.id===next.id?next:d);return next;}
  if(command==="delete_boat"){const index=docs.findIndex(d=>d.id===args!.boatId);deleted.push({doc:docs[index]!,index}); docs=docs.filter(d=>d.id!==args!.boatId);return docs[0];}
  if(command==="restore_boat"){const removed=deleted.pop()!;docs.splice(removed.index,0,removed.doc);return docs[0];}
  throw new Error(command);
 });
 host=document.createElement("div");toolbar=document.createElement("div");document.body.append(toolbar,host);root=createRoot(host);
 await act(async()=>root.render(<FleetHarness/>));
});
afterEach(async()=>{await act(async()=>root.unmount());host.remove();toolbar.remove();});
it("adds isolated tabs and sends rename and summary requests to the selected boat",async()=>{
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(1);
 await click('[data-feature="boats:add"]');
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(2);expect(active).toHaveBeenLastCalledWith(102);
 expect(toolbar.querySelector('[data-feature="boats:add"]')).not.toBeNull();
 expect(host.querySelector('[data-feature="boats:rename"]')).toBeNull();
 await act(async()=>host.querySelector('[role="tab"][aria-selected="true"]')!.dispatchEvent(new MouseEvent("dblclick",{bubbles:true})));
 const input=host.querySelector<HTMLInputElement>('[data-feature="boats:name"]')!;
 await act(async()=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,"value")!.set!.call(input,"Bravo");input.dispatchEvent(new Event("input",{bubbles:true}));});
 await act(async()=>input.dispatchEvent(new KeyboardEvent("keydown",{key:"Enter",bubbles:true})));
 expect(invoke).toHaveBeenCalledWith("rename_boat",{boatContext:102,name:"Bravo"});
 expect(docs[0]!.boat_name).toBe("Alpha");expect(docs[1]!.boat_name).toBe("Bravo");
 expect(host.querySelector('[data-feature="boats:export-all"]')).not.toBeNull();
});
it("populates two and four unique panes, forces 3D, and restores a single tab",async()=>{
 for(let i=0;i<3;i++)await click('[data-feature="boats:add"]');
 await click('[data-feature="boats:split"]');
 expect(host.querySelector('.boat-tabs-row')).toBeNull();
 expect(host.querySelectorAll('.boat-pane:not([hidden]) [data-mode="3d"]')).toHaveLength(2);
 await click('[data-feature="boats:four"]');
 expect(host.querySelector('.boat-tabs-row')).toBeNull();
 const panes=[...host.querySelectorAll('.boat-pane:not([hidden])')];expect(panes).toHaveLength(4);
 expect(new Set(panes.map(p=>p.getAttribute("data-boat-id"))).size).toBe(4);
 await click('[data-feature="boats:single"]');
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(4);
 expect(host.querySelectorAll('.boat-pane:not([hidden]) [data-mode="all"]')).toHaveLength(1);
});
it("cancels an inline rename and deletes/restores only the chosen boat", async()=>{
 expect(host.querySelector<HTMLButtonElement>('[data-feature="boats:delete"]')!.disabled).toBe(true);
 await click('[data-feature="boats:add"]');
 await act(async()=>host.querySelector('[role="tab"][aria-selected="true"]')!.dispatchEvent(new KeyboardEvent("keydown",{key:"F2",bubbles:true})));
 const input=host.querySelector<HTMLInputElement>('[data-feature="boats:name"]')!;
 await act(async()=>input.dispatchEvent(new KeyboardEvent("keydown",{key:"Escape",bubbles:true})));
 expect(invoke.mock.calls.filter(c=>c[0]==="rename_boat")).toHaveLength(0);
 await click('[data-feature="boats:delete"]');
 await click('[role="dialog"] button.danger');
 expect(invoke).toHaveBeenCalledWith("delete_boat",{projectId:101,boatId:102});
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(1);
 expect(host.querySelector('[data-boat-id="102"]')).toBeNull();
 await click('[data-feature="boats:restore"]');
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(2);
});
it("retains split panes when deleting and restoring the first boat", async()=>{
 await click('[data-feature="boats:add"]'); await click('[data-feature="boats:add"]');
 await click('[role="tab"]'); await click('[data-feature="boats:split"]');
 await click('[data-feature="boats:delete"]'); await click('[role="dialog"] button.danger');
 expect(docs[0]!.id).toBe(102);
 expect(host.querySelector('.boat-grid.boats-2')).not.toBeNull();
 expect(host.querySelector('.boat-tabs-row')).toBeNull();
 expect(host.querySelectorAll('.boat-pane:not([hidden])')).toHaveLength(2);
 expect(host.querySelector('[data-boat-id="101"]')).toBeNull();
 await click('[data-feature="boats:restore"]');
 expect(docs[0]!.id).toBe(101);
 expect(host.querySelectorAll('.boat-pane-picker option')).toHaveLength(6);
 expect(host.querySelector('.boat-tabs-row')).toBeNull();
});
/** What the MCP service's `document://changed` does: the shell hands down the root's newer summary. */
async function changedOutside(){docs=docs.map((d,i)=>i===0?{...d,revision:d.revision+1}:d);await act(async()=>setRoot(docs[0]!));await act(async()=>{});}
it("drops the pane of a boat removed from outside, and shows another without a failure", async()=>{
 await click('[data-feature="boats:add"]');
 expect(host.querySelector('.boat-pane.active')!.getAttribute("data-boat-id")).toBe("102");
 // An MCP client's boat_remove: the document changed under the interface.
 docs=docs.filter(d=>d.id!==102);
 await changedOutside();
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(1);
 expect(host.querySelector('[data-boat-id="102"]')).toBeNull();
 expect(host.querySelector('.boat-pane.active:not([hidden])')!.getAttribute("data-boat-id")).toBe("101");
 expect(active).toHaveBeenLastCalledWith(101);
 expect(reportFailure).not.toHaveBeenCalled();
});
it("shows a boat restored from outside after the interface deleted it", async()=>{
 await click('[data-feature="boats:add"]');
 await click('[data-feature="boats:delete"]'); await click('[role="dialog"] button.danger');
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(1);
 // An MCP client's boat_restore.
 const removed=deleted.pop()!;docs.splice(removed.index,0,removed.doc);
 await changedOutside();
 expect(host.querySelectorAll('[role="tab"]')).toHaveLength(2);
 await act(async()=>host.querySelectorAll<HTMLButtonElement>('[role="tab"]')[1]!.click());
 await act(async()=>{});
 const pane=host.querySelector('.boat-pane.active:not([hidden])')!;
 expect(pane.getAttribute("data-boat-id")).toBe("102");
 expect(pane.textContent).toContain("Boat 2");
 expect(reportFailure).not.toHaveBeenCalled();
});
