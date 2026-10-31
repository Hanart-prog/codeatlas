#!/usr/bin/env python3
"""Recomputes `codeatlas compare` from each commit's edges, with no shared code.

Usage: verify_compare.py <codeatlas binary> <repository>

Differences, cycles (Tarjan), degrees and dependents are worked out here from
`codeatlas graph`, and `git diff -M50%` checks direct renames. Exits non-zero on
any disagreement, except renames, where libgit2 and git score similarity
differently, which it reports.
"""
import json,subprocess,sys,os,random,tempfile
bin_,repo=sys.argv[1],sys.argv[2]
data=tempfile.mkdtemp()
def cli(*a):
    r=subprocess.run([bin_,*a,'--data-dir',data],capture_output=True,text=True)
    if r.returncode: raise SystemExit(r.stderr)
    return r.stdout
def git(*a): return subprocess.run(['git','-C',repo,'-c','core.quotePath=false',*a],capture_output=True,text=True,check=True).stdout
cli('analyze',repo,'--history','5000')
commits=git('rev-list','--first-parent','HEAD').split()
random.seed(7)
pairs=[(commits[-1],commits[0])]+[tuple(sorted(random.sample(commits,2),key=commits.index,reverse=True)) for _ in range(19)]
def graph(c):
    d=json.loads(cli('graph',repo,'--ref',c,'--limit','50000','--format','json'))
    return {n['path'] for n in d['nodes']},{(e['source'],e['target'],e['edge_type']) for e in d['edges']},d['total_node_count'],d['total_edge_count']
def sccs(nodes,adj):
    # iterative Tarjan
    idx={};low={};on=set();st=[];out=[];c=[0]
    for s in sorted(nodes):
        if s in idx: continue
        work=[(s,iter(sorted(adj.get(s,()))))]
        idx[s]=low[s]=c[0];c[0]+=1;st.append(s);on.add(s)
        while work:
            v,it=work[-1]
            for w in it:
                if w not in idx:
                    idx[w]=low[w]=c[0];c[0]+=1;st.append(w);on.add(w);work.append((w,iter(sorted(adj.get(w,())))));break
                elif w in on: low[v]=min(low[v],idx[w])
            else:
                work.pop()
                if work: low[work[-1][0]]=min(low[work[-1][0]],low[v])
                if low[v]==idx[v]:
                    comp=[]
                    while True:
                        w=st.pop();on.discard(w);comp.append(w)
                        if w==v:break
                    if len(comp)>1 or v in adj.get(v,()): out.append(frozenset(comp))
    return set(out)
def deps(nodes,edges):
    rev={}
    for s,t,_ in edges: rev.setdefault(t,set()).add(s)
    res={}
    for n in nodes:
        seen={n};q=[n]
        while q:
            x=q.pop()
            for y in rev.get(x,()):
                if y not in seen: seen.add(y);q.append(y)
        res[n]=len(seen)-1
    return res
tot=dict(pairs=0,edges=0,files=0,cycles=0,metrics=0,renames=0,gitdiffer=0)
for base,target in pairs:
    r=json.loads(cli('compare',repo,'--base',base,'--target',target,'--limit','5000','--format','json'))
    bn,be,bt,bet=graph(base);tn,te,tt,tet=graph(target)
    assert bt==len(bn) and tt==len(tn)
    # files: independent rename map from git CLI
    ren={}
    for l in git('diff','--name-status','-M50%','--no-ext-diff',base,target).splitlines():
        f=l.split('\t')
        if f[0].startswith('R') and f[1] in bn and f[2] in tn and f[1] not in tn and f[2] not in bn: ren[f[1]]=f[2]
    enginer={x['from']:x['to'] for x in r['renamed_files']}
    direct={x['from']:x['to'] for x in r['renamed_files'] if x['evidence']=='direct'}
    tot['history']=tot.get('history',0)+len(enginer)-len(direct)
    if ren!=direct:
        tot['gitdiffer']+=1
        print('  direct renames differ from git:',base[:8],len(ren),len(direct))
    ren=enginer  # edges below use the engine's renames; difference is reported above
    car=lambda p:ren.get(p,p)
    carried={(car(s),car(t),k) for s,t,k in be}
    add=te-carried; rem={(s,t,k) for s,t,k in be if (car(s),car(t),k) not in te}
    ea={(x['edge']['source'],x['edge']['target'],x['edge']['edge_type']) for x in r['added_imports']}
    er={(x['edge']['source'],x['edge']['target'],x['edge']['edge_type']) for x in r['removed_imports']}
    assert ea==add,(base,add^ea); assert er==rem,(base,rem^er)
    fa=tn-bn-set(ren.values()); fr=bn-tn-set(ren)
    assert {x['path'] for x in r['added_files']}==fa; assert {x['path'] for x in r['removed_files']}==fr
    def A(E):
        d={}
        for s,t,_ in E:d.setdefault(s,set()).add(t)
        return d
    cb={frozenset(map(car,c)) for c in sccs(bn,A(be))}; ct=sccs(tn,A(te))
    changed=r['changed_cycles']
    assert {frozenset(c['members']) for c in r['new_cycles']}|{frozenset(c['after']['members']) for c in changed}==ct-cb,base
    assert {frozenset(map(car,c['members'])) for c in r['resolved_cycles']}|{frozenset(map(car,c['before']['members'])) for c in changed}==cb-ct,base
    for c in changed:
        was,now=set(map(car,c['before']['members'])),set(c['after']['members'])
        assert was&now and set(c['joined'])==now-was and set(map(car,c['left']))==was-now,base
    db,dt2=deps(bn,be),deps(tn,te)
    def deg(E):
        i={};o={}
        for s,t in {(s,t) for s,t,_ in E}: i[t]=i.get(t,0)+1;o[s]=o.get(s,0)+1
        return i,o
    ib,ob=deg(be);it,ot=deg(te)
    exp={}
    for n in bn:
        m=car(n)
        if m in tn:
            v=(ib.get(n,0),it.get(m,0),ob.get(n,0),ot.get(m,0),db[n],dt2[m])
            if v[0]!=v[1] or v[2]!=v[3] or v[4]!=v[5]: exp[m]=v
    got={x['path']:(x['in_degree_before'],x['in_degree_after'],x['out_degree_before'],x['out_degree_after'],x['dependents_before'],x['dependents_after']) for x in r['metric_changes']}
    assert got==exp,base
    tot['pairs']+=1;tot['edges']+=len(add)+len(rem);tot['files']+=len(fa)+len(fr);tot['cycles']+=len(ct-cb)+len(cb-ct);tot['metrics']+=len(exp);tot['renames']+=len(ren)
    print(f"  ok {base[:8]}..{target[:8]} files {len(bn)}->{len(tn)} edges {len(be)}->{len(te)} +{len(add)}/-{len(rem)} cycles {len(cb)}->{len(ct)}")
print(os.path.basename(repo),tot)
assert tot['pairs']>0
