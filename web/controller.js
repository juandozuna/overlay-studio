'use strict';
(() => {
const get=id=>document.getElementById(id);
const params=new URLSearchParams(location.hash.slice(1));
let token=params.get('token')||sessionStorage.getItem('overlay-token')||'';
if(params.has('token')){sessionStorage.setItem('overlay-token',token);history.replaceState(null,'',location.pathname);}
const headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'};
let online=false,busy=false,layoutKey='',pendingButton=null;
const client=Array.from(crypto.getRandomValues(new Uint8Array(16)),v=>v.toString(16).padStart(2,'0')).join('');let seq=0;
window.addEventListener('hashchange',()=>{
 const next=new URLSearchParams(location.hash.slice(1)).get('token');
 if(next){token=next;headers.Authorization='Bearer '+token;sessionStorage.setItem('overlay-token',token);history.replaceState(null,'',location.pathname);poll();}
});
function availability(){get('controls').querySelectorAll('button').forEach(b=>b.disabled=!online||busy);}
function text(id,value){get(id).textContent=value;}
async function send(button){if(!online||busy)return;busy=true;availability();text('message','Sending…');
 try{const response=await fetch('/api/action',{method:'POST',headers,body:JSON.stringify({button:button.id,request_id:client+':'+(++seq)}),signal:AbortSignal.timeout(5000)});if(!response.ok)throw new Error(await response.text());text('message',button.label+' applied');await poll();}
 catch(error){text('message','Could not confirm action: '+error.message+'. Check the live score before trying again.');}
 finally{busy=false;availability();}
}
function click(button){if(button.confirm){pendingButton=button;text('confirm-label',button.label+'?');get('confirm').showModal();}else send(button);}
get('cancel').onclick=()=>{pendingButton=null;get('confirm').close();};get('proceed').onclick=()=>{get('confirm').close();if(pendingButton)send(pendingButton);pendingButton=null;};
async function poll(){try{const response=await fetch('/api/state',{headers,cache:'no-store',signal:AbortSignal.timeout(2500)});if(!response.ok)throw new Error(response.status===401?'Scan the current QR code to pair':'Desktop unavailable');const state=await response.json();online=true;text('connection','Connected');text('project',state.project);text('round','Round '+(state.values.round||'—'));text('layout-name',state.layout.name);
 get('teams').replaceChildren(...state.teams.map(team=>{const card=document.createElement('div');card.className='team';card.style.setProperty('--team','rgb('+team.color.join(',')+')');const name=document.createElement('span');name.textContent=team.name||team.label;const score=document.createElement('strong');score.textContent=team.score;card.append(name,score);return card;}));
 get('clocks').replaceChildren(...state.clocks.map(clock=>{const card=document.createElement('div');card.className='clock';const value=document.createElement('strong');value.textContent=clock.value;const name=document.createElement('span');name.textContent=clock.name+(clock.running?' · Running':' · Paused');card.append(value,name);return card;}));
 const key=JSON.stringify(state.layout);if(key!==layoutKey){layoutKey=key;get('controls').replaceChildren();for(const control of state.layout.controls){const b=document.createElement('button');b.textContent=control.label;b.style.gridColumn=(control.col+1)+' / span '+control.width;b.style.gridRow=(control.row+1)+' / span '+control.height;b.style.backgroundColor='rgb('+control.color.join(',')+')';b.onclick=()=>click(control);get('controls').append(b);}}
 get('outputs').replaceChildren(...state.outputs.map(label=>{const p=document.createElement('span');p.textContent=label;return p;}));availability();
 }catch(error){online=false;text('connection','Disconnected');text('message',token?error.message:'Scan the QR code shown in the desktop app.');availability();}}
async function loop(){await poll();setTimeout(loop,500);}loop();document.addEventListener('visibilitychange',()=>{if(!document.hidden)poll();});
})();
