"""Executable numerical oracle and adversarial tests for the Arclet jet patch.

This is a scalar Python reproduction of the Rust arithmetic, NOT a Rust build or
execution. Independent references are SciPy's compiled B-spline evaluator and
90-decimal mpmath differentiation of the rational Bernstein formula.
"""
from __future__ import annotations
import json, math, random, time
from pathlib import Path
import numpy as np
from scipy.interpolate import BSpline
import mpmath as mp

mp.mp.dps = 90
SEED = 20261101

class Compensated:
    def __init__(self): self.sum = self.correction = 0.0
    def add(self, value):
        total = self.sum + value
        self.correction += ((self.sum - total) + value) if abs(self.sum) >= abs(value) else ((value - total) + self.sum)
        self.sum = total
    def total(self): return self.sum + self.correction

def local_basis(knots, u, left, n):
    p = len(knots) - 2
    out = [0.0] * 8
    if not 0 <= p <= 7 or not math.isfinite(u): return out
    levels = min(n, p)
    table = [[0.0] * 8 for _ in range(8)]
    for j in range(p + 1):
        a, b = knots[j:j+2]
        table[0][j] = float(a < u <= b if left else a <= u < b)
    def ratio(x, d): return x/d if d > 0.0 else 0.0
    for q in range(1, p+1):
        for k in range(levels, -1, -1):
            for j in range(p-q+1):
                d1, d2 = knots[j+q]-knots[j], knots[j+q+1]-knots[j+1]
                if k == 0:
                    value = ratio(u-knots[j],d1)*table[0][j]+ratio(knots[j+q+1]-u,d2)*table[0][j+1]
                else:
                    value = q*(ratio(table[k-1][j],d1)-ratio(table[k-1][j+1],d2))
                table[k][j] = value
    for k in range(levels+1): out[k] = table[k][0]
    return out

def basis_values(knots, degree, t, order=3):
    count = len(knots)-degree-1
    if not math.isfinite(t) or not knots[degree] <= t <= knots[count]:
        raise ValueError('outside domain')
    span = count-1 if t == knots[count] else int(np.searchsorted(knots,t,side='right'))-1
    first = span-degree
    values = [[local_basis(knots[i:i+degree+2],t,t==knots[count],order)[k] for i in range(first,span+1)] for k in range(order+1)]
    return first, values

def finite_jet(result):
    if not all(math.isfinite(x) for v in result for x in v): raise ValueError('overflow')
    return np.array(result)

def evaluate(points, weights, degree, knots, t, *, stable=True, order=3):
    first, values = basis_values(knots,degree,t,0 if stable else order)
    points = [[float(x) for x in p] for p in points[first:first+degree+1]]
    weights = [float(w) for w in weights[first:first+degree+1]]
    # The Rust patch takes the same exact constant-span shortcut.
    if stable and all(p == points[0] for p in points):
        return np.array([points[0]]+[[0.0]*3 for _ in range(order)])
    if stable and order:
        _, values = basis_values(knots,degree,t,order)
    max_weight = max(weights)
    weight_sum = sum((w/max_weight)*v for w,v in zip(weights,values[0]))
    if not weight_sum > 0 or not math.isfinite(weight_sum): raise ValueError('weight')
    wn = [(w/max_weight)/weight_sum for w in weights]
    if not all(math.isfinite(w) for w in wn): raise ValueError('weight ratio')
    origin_index = max(range(len(points)),key=lambda i:wn[i]*values[0][i])
    origin = points[origin_index] if stable else [0.0]*3
    half = stable and any(not math.isfinite(p[j]-origin[j]) for p in points for j in range(3))
    numerators=[]; denominators=[]
    for k in range(order+1):
        acc = [Compensated() for _ in range(3)]
        den = Compensated()
        basic=[0.0]*3;basic_w=0.0
        for p,w,b in zip(points,wn,values[k]):
            factor=w*b
            if stable and factor==0.0: continue
            local=[p[j]*0.5-origin[j]*0.5 if half else p[j]-origin[j] for j in range(3)]
            for j in range(3):
                term=local[j]*factor
                if stable: acc[j].add(term)
                else: basic[j]+=term
            if stable: den.add(factor)
            else: basic_w+=factor
        numerators.append([a.total() for a in acc] if stable else basic)
        denominators.append(den.total() if stable else basic_w)
    result=[]
    for k in range(order+1):
        v=numerators[k][:]
        for i in range(1,k+1):
            scale=math.comb(k,i)*denominators[i]
            v=[v[j]-result[k-i][j]*scale for j in range(3)]
        result.append([x/denominators[0] for x in v] if stable else v)
    if stable:
        result[0]=[(origin[j]*0.5+result[0][j])*2 if half else origin[j]+result[0][j] for j in range(3)]
        if half:
            result[1:]=[[2*x for x in v] for v in result[1:]]
    return finite_jet(result)

def bernstein_reference(points, weights, degree, t):
    pts=[[mp.mpf(float(x)) for x in pt] for pt in points];w=[mp.mpf(float(x)) for x in weights]
    t=mp.mpf(float(t))
    def coordinate(j):
        def f(u):
            terms=[mp.binomial(degree,i)*u**i*(1-u)**(degree-i)*w[i] for i in range(degree+1)]
            return mp.fsum(terms[i]*pts[i][j] for i in range(degree+1))/mp.fsum(terms)
        return f
    return np.array([[float(mp.diff(coordinate(j),t,k)) for j in range(3)] for k in range(4)])

def scipy_reference(points,weights,p,knots,t):
    # Anchored high-quality vectorized reference, independently compiled basis arithmetic.
    pts=np.asarray(points); w=np.asarray(weights)
    b=BSpline(knots,np.eye(len(points)),p)
    values=[b(t,nu=k) if k<=p else np.zeros(len(points)) for k in range(4)]
    origin=pts[np.argmax(w*values[0])]
    weighted=(pts-origin)*w[:,None]
    num=[np.array([math.fsum(weighted[:,j]*v) for j in range(3)]) for v in values]
    den=[math.fsum(w*v) for v in values]
    result=[]
    for k in range(4):
        v=num[k].copy()
        for i in range(1,k+1):v-=math.comb(k,i)*den[i]*result[k-i]
        result.append(v/den[0])
    result[0]+=origin
    return np.array(result)

def arc(jet):
    d1,d2,d3=jet[1:]
    v=math.hypot(*d1)
    if not v>0 or not math.isfinite(v):raise ValueError('stationary')
    T=d1/v; A=(d2/v)/v; J=((d3/v)/v)/v
    h=np.dot(T,A); K=A-T*h
    D=J-3*h*K-T*(np.dot(T,J)+np.dot(K,K))
    return np.array([T,K,D])

def run():
    rng=random.Random(SEED); cases=[]; failures=[]; total=0
    def check(name, condition, evidence=None):
        nonlocal total
        total+=1
        if not bool(condition): failures.append(dict(name=name,evidence=evidence))
    start=time.perf_counter()
    circle=np.array([[1.,0,0],[1,1,0],[0,1,0]]); weights=[1.,1/math.sqrt(2),1.]; knots=[0.,0.,0.,1.,1.,1.]
    # Multiprecision independent oracle, including large translations and exact endpoints.
    for offset in [0.,1e6,1e9,1e12,1e14]:
        pts=circle+np.array([offset,-offset,offset]); olderr=np.zeros(4);newerr=np.zeros(4)
        for t in [0.,.03,.19,.37,.5,.77,.97,1.]:
            ref=bernstein_reference(pts,weights,2,t)
            old=evaluate(pts,weights,2,knots,t,stable=False);new=evaluate(pts,weights,2,knots,t)
            olderr=np.maximum(olderr,np.max(abs(old-ref),axis=1));newerr=np.maximum(newerr,np.max(abs(new-ref),axis=1))
            check(f'circle derivatives translation={offset} t={t}',np.max(abs(new[1:]-ref[1:]))<2e-12)
            check(f'circle curvature translation={offset} t={t}',abs(np.linalg.norm(arc(new)[1])-1)<2e-12)
        cases.append(dict(translation=offset,old_max_abs_per_order=olderr.tolist(),new_max_abs_per_order=newerr.tolist()))
    const=np.tile([1e12,-2e12,3e12],(3,1))
    for i in range(101):
        j=evaluate(const,weights,2,knots,i/100)
        check('constant no phantom tangent',np.array_equal(j[1:],np.zeros((3,3))))
        check('constant point exact',np.array_equal(j[0],const[0]))
        try:arc(j);check('constant rejected by continuity',False)
        except ValueError:check('constant rejected by continuity',True)
    # Degree, nonuniform/repeated knots, endpoints, rational/nonrational random fixtures.
    max_basis_relative=0.;max_jet_relative=0.
    for case in range(240):
        p=rng.randint(1,7); n=rng.randint(p+1,p+9)
        interior=sorted(rng.uniform(.01,.99) for _ in range(n-p-1))
        if len(interior)>1 and p>1 and case%3==0:interior[1]=interior[0]
        kv=[0.]*(p+1)+interior+[1.]*(p+1)
        pts=np.array([[rng.uniform(-20,20) for _ in range(3)] for _ in range(n)])
        ww=[10**rng.uniform(-2,2) for _ in range(n)] if case%2 else [1.]*n
        sp=BSpline(kv,np.eye(n),p)
        for t in [0.,1.,rng.random(),rng.random()]+interior[:2]:
            first, vals=basis_values(kv,p,t)
            for k in range(4):
                got=np.zeros(n);got[first:first+p+1]=vals[k]
                expected=sp(t,nu=k) if k<=p else np.zeros(n)
                rel=np.max(abs(got-expected))/max(1.,np.max(abs(expected)))
                max_basis_relative=max(max_basis_relative,float(rel))
                check('basis scipy agreement',rel<2e-12,dict(case=case,t=t,k=k,relative=float(rel)))
            actual=evaluate(pts,ww,p,kv,t); ref=scipy_reference(pts,ww,p,kv,t)
            rels=np.max(abs(actual-ref),axis=1)/np.maximum(1.,np.max(abs(ref),axis=1))
            max_jet_relative=max(max_jet_relative,float(max(rels)))
            check('rational jet scipy agreement',max(rels)<2e-9,dict(case=case,t=t,relative=rels.tolist()))
            # Translation invariance is checked only at exactly representable controls.
    for p in range(1,8):
        for case in range(5):
            pts=[[float(rng.randint(-20,20)) for _ in range(3)] for _ in range(p+1)]
            ww=[10**rng.uniform(-1,1) for _ in pts];kv=[0.]*(p+1)+[1.]*(p+1);t=rng.uniform(.05,.95)
            ref=bernstein_reference(pts,ww,p,t);got=evaluate(pts,ww,p,kv,t)
            check('random Bernstein high precision',np.max(abs(got-ref)/np.maximum(1.,abs(ref)))<2e-9)
            translated=(np.array(pts)+[1e12,-1e12,1e12]).tolist()
            check('integer controls translation invariance',np.allclose(evaluate(translated,ww,p,kv,t)[1:],got[1:],rtol=2e-13,atol=2e-10))
    for scale in [1e-200,1e200]:
        for i in range(41):
            a=evaluate(circle,weights,2,knots,i/40);b=evaluate(circle,[w*scale for w in weights],2,knots,i/40)
            check('common weight scale',np.max(abs(a-b))<1e-11)
    for a,b,t in [(-1e308,1e308,2.),(1e308,-1e308,2.)]:
        got=evaluate([[a,0,0],[b,0,0]],[1.,1.],1,[0.,0.,4.,4.],t)
        check('finite extremes midpoint',np.array_equal(got[0],np.zeros(3)))
        check('finite extremes derivative',abs(got[1,0]/((b*.5-a*.5)/2)-1)<1e-14)
    for reverse in [False,True]:
        pts=[[1e-230,0,0],[1.,0,0]]; ww=[1e-100,1.]
        if reverse:pts.reverse();ww.reverse()
        for t in [0.,1.]:
            got=evaluate(pts,ww,1,[0.,0.,1.,1.],t,order=0)
            check('tiny endpoint exact',np.array_equal(got[0],pts[int(t)]))
    # The constant-span shortcut runs before enormous basis derivatives are formed.
    for p in range(1,8):
        tiny_knots=[0.]*(p+1)+[1e-200]*(p+1)
        controls=[[1e300,-1e300,0.]]*(p+1)
        ww=[1e-100 if i%2 else 1e100 for i in range(p+1)]
        for t in [0.,3.7e-201,1e-200]:
            j=evaluate(controls,ww,p,tiny_knots,t)
            check('constant tiny domain exact position',np.array_equal(j[0],controls[0]))
            check('constant tiny domain zero derivatives',np.array_equal(j[1:],np.zeros((3,3))))
    for t in [math.nan,math.inf,-math.inf,-1.,2.]:
        try:evaluate(circle,weights,2,knots,t);check('invalid parameter rejected',False)
        except ValueError:check('invalid parameter rejected',True)
    # Geometric continuity must be invariant under a non-linear orientation-preserving
    # reparameterization. Apply chain rules at one point, not a visual approximation.
    for _ in range(150):
        jet=evaluate(circle,weights,2,knots,rng.uniform(.02,.98)); a=10**rng.uniform(-5,5); b=rng.uniform(-5,5); c=rng.uniform(-5,5)
        transformed=np.array([jet[0],jet[1]*a,jet[2]*a*a+jet[1]*b,jet[3]*a*a*a+3*jet[2]*a*b+jet[1]*c])
        # Very small a and large b are ill-conditioned; bound the test away from
        # destructive chain-rule cancellation instead of claiming arbitrary precision.
        if abs(b)/a**2<1e4 and abs(c)/a**3<1e4:
            check('arc length jet reparameterization',np.max(abs(arc(jet)-arc(transformed)))<5e-8)
    report=dict(scope='Python arithmetic reproduction and independent oracles; NOT compiled Rust tests',seed=SEED,mpmath_digits=mp.mp.dps,
                assertions=total,failed=len(failures),failures=failures,translations=cases,
                max_basis_scaled_error=max_basis_relative,max_jet_scaled_error=max_jet_relative,elapsed_seconds=time.perf_counter()-start)
    Path(__file__).with_name('verification.json').write_text(json.dumps(report,indent=2))
    print(json.dumps({k:v for k,v in report.items() if k not in ['translations','failures']},indent=2))
    if failures: print(json.dumps(failures[:10],indent=2)); raise SystemExit(1)
if __name__=='__main__':run()
