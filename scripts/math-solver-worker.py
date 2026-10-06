#!/usr/bin/env python3
"""Offline CPU math service: safe parsing, explicit teaching rules and plotting.

JSON lines only on stdout. Runs independently of Torch and the OCR model.
"""
import argparse
from dataclasses import fields, is_dataclass
import json
import math
import os
from pathlib import Path
import re
import resource
import signal
import socket
import sys
from functools import lru_cache

from math_parser import Compiler, MathInputError, Node, parse

base_socket = socket.socket
class OfflineSocket(base_socket):
    def __init__(self, family=socket.AF_INET, *args, **kwargs):
        if family in (socket.AF_INET, socket.AF_INET6):
            raise OSError('Network access is disabled in Folio math solving')
        super().__init__(family, *args, **kwargs)
socket.socket = OfflineSocket

import sympy as s
from sympy.core.relational import Equality, Relational
from sympy.integrals.manualintegrate import integral_steps
from sympy.solvers.solveset import NonlinearError

VERSION = 'folio-math-1'
OPERATIONS = {'auto', 'evaluate', 'simplify', 'factor', 'solve', 'differentiate',
              'integrate', 'graph', 'check', 'interpret', 'live', 'assign'}

def latex(value):
    return s.latex(value, fold_short_frac=False)

def normal(value):
    return s.simplify(value)

def restrictions(conditions):
    return [latex(condition) for condition in conditions if condition is not s.S.true]

def check_conditions(value, variable, conditions):
    for condition in conditions:
        substituted = normal(condition.subs(variable, value))
        if substituted is s.S.false:
            return False
        if substituted is not s.S.true:
            return None
    return True

def solution_set(equation, variable, compiler):
    domain = s.S.Reals if compiler.domain == 'real' else s.S.Complexes
    result = s.solveset(equation.lhs-equation.rhs, variable, domain=domain)
    if isinstance(result, s.FiniteSet):
        keep = []
        for root in result:
            condition = check_conditions(root, variable, compiler.conditions)
            if condition is None:
                raise MathInputError('The solution depends on unresolved domain conditions. Supply the parameter values.')
            residual = normal((equation.lhs-equation.rhs).subs(variable, root))
            if condition and residual == 0:
                keep.append(root)
            elif condition and residual.is_zero is None:
                raise MathInputError('Could not verify a candidate solution in the original equation.')
        return s.FiniteSet(*keep)
    if isinstance(result, s.ConditionSet):
        raise MathInputError('No complete symbolic solution is available for this equation.')
    for condition in compiler.conditions:
        if condition.free_symbols <= {variable}:
            try:
                permitted = s.solve_univariate_inequality(condition, variable, relational=False)
                result = s.Intersection(result, permitted)
            except (NotImplementedError, ValueError):
                raise MathInputError('Could not establish the complete domain for this equation.')
    return result

def step(rule, before, after, explanation, details='', status='verified', children=None):
    return {'rule': rule, 'before': latex(before) if not isinstance(before, str) else before,
            'after': latex(after) if not isinstance(after, str) else after,
            'explanation': explanation, 'details': details, 'status': status,
            'children': children or []}

def simplify_steps(expression, operation):
    steps = []
    current = expression
    for rule, calculate, explanation in [
            ('distribute', s.expand, 'Distribute multiplication over addition.'),
            ('combine_fractions', s.together, 'Write the fractions over a common denominator.'),
            ('cancel_factors', s.cancel, 'Cancel common factors, retaining the original restrictions.'),
            ('simplify', s.simplify, 'Evaluate arithmetic and combine like terms.')]:
        after = calculate(current)
        if s.srepr(after) != s.srepr(current):
            if normal(after-current) != 0:
                continue
            steps.append(step(rule, current, after, explanation))
            current = after
    if operation == 'factor':
        after = s.factor(current)
        if s.srepr(after) != s.srepr(current):
            steps.append(step('factor', current, after, 'Factor the expression into a product.',
                              'Expand the product to check that it equals the original expression.'))
        current = after
    if not steps:
        steps.append(step('already_simplified', expression, current, 'The expression is already simplified.'))
    return current, steps

def equation_steps(equation, variable, compiler, method):
    answer = solution_set(equation, variable, compiler)
    current = equation
    steps = []
    lhs, rhs = normal(current.lhs), normal(current.rhs)
    reduced = s.Eq(lhs, rhs, evaluate=False)
    if s.srepr(reduced) != s.srepr(current):
        steps.append(step('combine_terms', current, reduced, 'Simplify each side and combine like terms.'))
        current = reduced
    difference = s.expand(lhs-rhs)
    try:
        polynomial = s.Poly(difference, variable)
    except s.PolynomialError:
        polynomial = None
    methods = []
    if polynomial and polynomial.degree() == 1:
        a, b = polynomial.all_coeffs()
        if a.free_symbols or b.free_symbols:
            raise MathInputError('Give values for the parameters before solving this equation.')
        normalized = s.Eq(a*variable+b, 0, evaluate=False)
        if s.srepr(current) != s.srepr(normalized):
            steps.append(step('move_terms', current, normalized, 'Move all terms to the left side.'))
        isolated = s.Eq(a*variable, -b, evaluate=False)
        if b != 0:
            steps.append(step('subtract_constant', normalized, isolated,
                              f'Subtract {s.sstr(b)} from both sides.'))
        solved = s.Eq(variable, -b/a, evaluate=False)
        if a != 1:
            steps.append(step('divide_coefficient', isolated, solved,
                              f'Divide both sides by the nonzero coefficient {s.sstr(a)}.'))
        methods = ['isolate_variable']
    elif polynomial and polynomial.degree() == 2:
        a, b, c = polynomial.all_coeffs()
        if any(coefficient.free_symbols for coefficient in (a,b,c)):
            raise MathInputError('Give values for the parameters before solving this quadratic.')
        standard = s.Eq(difference, 0, evaluate=False)
        if s.srepr(current) != s.srepr(standard):
            steps.append(step('standard_form', current, standard, 'Write the quadratic in standard form.'))
        factored = s.factor(difference)
        methods = ['quadratic_formula', 'complete_square']
        if isinstance(factored, s.Mul) or isinstance(factored, s.Pow):
            methods.insert(0, 'factor')
        chosen = method if method in methods else methods[0]
        if chosen == 'factor':
            steps.append(step('factor_quadratic', standard, s.Eq(factored, 0, evaluate=False),
                              'Factor the quadratic.', 'A product is zero when at least one factor is zero.'))
        elif chosen == 'complete_square':
            h = b/(2*a)
            square = s.Eq((variable+h)**2, h*h-c/a, evaluate=False)
            steps.append(step('complete_square', standard, square,
                              'Divide by the leading coefficient and complete the square.',
                              'Add the square of half the linear coefficient to both sides.'))
            steps.append(step('square_roots', square, latex(answer),
                              'Take both square-root branches and isolate the variable.'))
        else:
            discriminant = b*b-4*a*c
            steps.append(step('discriminant', standard, s.Eq(s.Symbol('D'), discriminant, evaluate=False),
                              'Compute the discriminant b² − 4ac.', f'a={a}, b={b}, c={c}.'))
            roots = r'x = \frac{-b \pm \sqrt{b^2-4ac}}{2a}'
            roots = roots.replace('x =', latex(variable)+' =')
            steps.append(step('quadratic_formula', roots, latex(answer),
                              'Substitute the coefficients into the quadratic formula.',
                              'Include both signs. Over the real numbers, a negative discriminant gives no roots.'))
        steps.append(step('root_candidates', standard, latex(answer), 'Collect the roots in the selected domain.'))
    else:
        # Explain supported equation strategies without presenting a fabricated full trace.
        numerator, denominator = s.fraction(s.together(difference))
        if denominator != 1:
            steps.append(step('clear_denominators', equation, s.Eq(numerator, 0, evaluate=False),
                              'Clear denominators on the permitted domain.',
                              'Values making any original denominator zero remain excluded.', status='conditional'))
        radical = equation.lhs.has(s.Pow) or equation.rhs.has(s.Pow)
        steps.append(step('symbolic_roots', equation, latex(answer),
                          'Compute the roots and test them in the original equation.',
                          'A complete teaching trace for this equation type is not available; this step summarizes symbolic solving.',
                          status='summary'))
        methods = ['symbolic']
    if isinstance(answer, s.FiniteSet):
        for root in sorted(answer, key=s.default_sort_key):
            residual = normal((equation.lhs-equation.rhs).subs(variable, root))
            steps.append(step('substitute_check', s.Eq(variable, root, evaluate=False),
                              s.Eq(equation.lhs.subs(variable, root), equation.rhs.subs(variable, root), evaluate=False),
                              'Substitute this root into the original equation and check the restrictions.',
                              f'Left side minus right side is {residual}.'))
    if answer is s.S.EmptySet:
        steps.append(step('no_solution', equation, r'\varnothing', 'There are no solutions in the selected domain.'))
    return answer, steps, methods

def row_reduction_steps(matrix):
    current=matrix.copy();steps=[];row=0
    for column in range(current.cols-1):
        pivot=next((index for index in range(row,current.rows) if current[index,column]!=0),None)
        if pivot is None: continue
        if pivot!=row:
            before=current.copy();current.row_swap(row,pivot)
            steps.append(step('swap_rows',before,current,f'Swap rows {row+1} and {pivot+1}.'))
        coefficient=current[row,column]
        if coefficient!=1:
            before=current.copy();current.row_op(row,lambda value,_:normal(value/coefficient))
            steps.append(step('scale_row',before,current,f'Divide row {row+1} by {s.sstr(coefficient)}.'))
        for target in range(current.rows):
            coefficient=current[target,column]
            if target==row or coefficient==0: continue
            before=current.copy();current.row_op(target,lambda value,index:normal(value-coefficient*current[row,index]))
            steps.append(step('eliminate_entry',before,current,
                              f'Subtract {s.sstr(coefficient)} times row {row+1} from row {target+1}.'))
        row+=1
        if row>=current.rows:break
    return current,steps

def inequality_steps(expression,variable,compiler):
    answer=s.reduce_inequalities([expression]+compiler.conditions,variable)
    difference=s.expand(expression.lhs-expression.rhs)
    steps=[]
    try: polynomial=s.Poly(difference,variable)
    except s.PolynomialError: polynomial=None
    relation=type(expression)
    if polynomial and polynomial.degree()==1:
        a,b=polynomial.all_coeffs()
        standard=relation(a*variable+b,0,evaluate=False)
        steps.append(step('move_terms',expression,standard,'Move all terms to the left and combine like terms.'))
        isolated=relation(a*variable,-b,evaluate=False)
        steps.append(step('subtract_constant',standard,isolated,f'Subtract {s.sstr(b)} from both sides.'))
        flipped={'<':s.Gt,'>':s.Lt,'<=':s.Ge,'>=':s.Le,'!=':s.Ne}.get(expression.rel_op,relation)
        solved=(flipped if a.is_negative else relation)(variable,normal(-b/a),evaluate=False)
        steps.append(step('divide_inequality',isolated,solved,
                          f'Divide by {s.sstr(a)}'+('; reverse the inequality because it is negative.' if a.is_negative else '.')))
        steps.append(step('apply_domain',solved,answer,'Intersect with the original domain restrictions.'))
    elif polynomial:
        factored=s.factor(difference)
        roots=s.solveset(difference,variable,domain=s.S.Reals)
        steps.append(step('factor_inequality',expression,relation(factored,0,evaluate=False),'Factor the polynomial to find where its sign can change.'))
        steps.append(step('critical_points',factored,roots,'Find the real zeros that divide the number line into intervals.'))
        if isinstance(roots,s.FiniteSet):
            ordered=sorted(roots,key=s.default_sort_key)
            boundaries=[-s.oo]+ordered+[s.oo]
            for left,right in zip(boundaries,boundaries[1:]):
                point=right-1 if left is -s.oo else left+1 if right is s.oo else (left+right)/2
                test=s.simplify(expression.subs(variable,point))
                steps.append(step('test_interval',s.Interval.open(left,right),s.Eq(variable,point,evaluate=False),
                                  f'Test this interval: the inequality is {"true" if test is s.S.true else "false"}.'))
            steps.append(step('choose_intervals',expression,answer,'Keep the true intervals and include endpoints only when the inequality permits equality.'))
        else:
            steps.append(step('interval_solution',expression,answer,'Find the satisfying intervals.',status='summary'))
    else:
        steps.append(step('inequality_solution',expression,answer,'Find the satisfying intervals and apply domain restrictions.',status='summary'))
    return answer,steps

def derivative_steps(expression, variable):
    if not expression.has(variable):
        return [step('constant_rule', s.Derivative(expression, variable), s.Integer(0), 'The derivative of a constant is zero.')]
    derivative = s.diff(expression, variable)
    if isinstance(expression, s.Add):
        children = [child for term in expression.args for child in derivative_steps(term, variable)]
        rule, explanation = 'sum_rule', 'Differentiate each term and add the derivatives.'
    elif isinstance(expression, s.Mul):
        children = [child for factor in expression.args if factor.has(variable)
                    for child in derivative_steps(factor, variable)]
        rule, explanation = 'product_rule', 'Apply the product rule to the factors.'
    elif isinstance(expression, s.Pow) and not expression.exp.has(variable):
        children = derivative_steps(expression.base, variable) if expression.base != variable else []
        rule, explanation = 'power_chain_rule', 'Apply the power rule and the chain rule when the base is a function.'
    elif expression.is_Function and len(expression.args) == 1:
        children = derivative_steps(expression.args[0], variable) if expression.args[0] != variable else []
        rule, explanation = 'function_chain_rule', 'Differentiate the outer function and multiply by the inner derivative.'
    else:
        return [step('symbolic_derivative', s.Derivative(expression, variable), derivative,
                     'Compute the symbolic derivative.', status='summary')]
    return [step(rule, s.Derivative(expression, variable), derivative, explanation, children=children)]

def integration_steps(expression, variable, domain):
    rule = integral_steps(expression, variable)
    descriptions = {
        'ConstantRule': 'Integrate a constant by multiplying it by the variable.',
        'PowerRule': 'Increase the power by one and divide by the new power.',
        'AddRule': 'Integrate each term separately.',
        'ConstantTimesRule': 'Take the constant factor outside the integral.',
        'URule': 'Substitute a new variable and include the differential factor.',
        'PartsRule': 'Use integration by parts: ∫u dv = uv − ∫v du.',
        'ExpRule': 'Apply the exponential integration rule.',
        'SinRule': 'Apply the sine integration rule.',
        'CosRule': 'Apply the cosine integration rule.',
        'ReciprocalRule': 'Integrate the reciprocal using a logarithm.',
        'ArctanRule': 'Recognize the derivative of arctangent.',
        'RewriteRule': 'Rewrite the integrand into a form with a known integration rule.',
        'AlternativeRule': 'Choose an available integration method.',
    }
    def convert(item, depth=0):
        if depth > 12:
            return []
        children = []
        if is_dataclass(item):
            for field in fields(item):
                value = getattr(item, field.name)
                if hasattr(value, 'integrand') and hasattr(value, 'eval'):
                    children.extend(convert(value, depth+1))
                elif isinstance(value, (list,tuple)):
                    for child in value:
                        if hasattr(child, 'integrand') and hasattr(child, 'eval'):
                            children.extend(convert(child, depth+1))
        name = type(item).__name__
        result = item.eval()
        if result.has(s.Integral):
            raise MathInputError('A complete manual integration method is not available for this integral.')
        return [step(name, s.Integral(item.integrand, item.variable), result,
                     descriptions.get(name, 'Apply this supported integration rule.'),
                     status='verified' if name in descriptions else 'summary', children=children)]
    result = rule.eval()
    steps = convert(rule)
    if normal(s.diff(result, variable)-expression) != 0:
        raise MathInputError('Could not verify this antiderivative by differentiation.')
    steps.append(step('check_antiderivative', s.Derivative(result, variable), expression,
                      'Differentiate the result to check the antiderivative.'))
    if domain == 'real':
        def real_logs(value):
            return value.replace(lambda term: term.func == s.log and len(term.args)==1 and term.args[0].is_real is True,
                                 lambda term: s.log(s.Abs(term.args[0])))
        real_result=real_logs(result)
        if real_result != result:
            # d log|g(x)| = g'(x)/g(x) on each real interval where g != 0.
            # The analytic rule was checked above before this domain-specific rewrite.
            steps.append(step('real_logarithm',result,real_result,
                              'For real integrals, use the logarithm of the absolute value.',
                              'Valid on each interval where every logarithm argument is nonzero.',status='conditional'))
            result=real_result

    steps.append(step('integration_constant', result, result+s.Symbol('C'), 'Add the arbitrary constant of integration.'))
    return result+s.Symbol('C'), steps

def interpret_words(text):
    """Deterministic local interpretations; every translation is reviewed in UI."""
    text = text.strip().lower().rstrip('?.')
    number = r'(-?\d+(?:\.\d+)?)'
    match = re.fullmatch(r'(?:what is |calculate |find )?'+number+r'\s*(?:%|percent)\s+of\s+'+number, text)
    if match:
        a,b = match.groups(); return f'{a}/100*{b}', 'percentage', 'Multiply the whole amount by the percentage divided by 100.'
    match = re.fullmatch(r'(?:increase|decrease)\s+'+number+r'\s+by\s+'+number+r'\s*(?:%|percent)',text)
    if match:
        a,b=match.groups(); sign='+' if text.startswith('increase') else '-'
        return f'{a}*(1{sign}{b}/100)', 'percentage_change', 'Apply the percentage change to the original amount.'
    match = re.search(r'(twice|three times|half of|double)\s+(?:a |the )?number\s+(plus|minus|increased by|decreased by)\s+'+number+r'\s+(?:is|equals|equal to)\s+'+number, text)
    if match:
        multiplier,operation,a,b=match.groups()
        k={'twice':'2','three times':'3','half of':'1/2','double':'2'}[multiplier]
        sign='+' if operation in ('plus','increased by') else '-'
        return f'({k})*x{sign}{a}={b}', 'linear_word_problem', 'Let x be the unknown number and translate the stated operations.'
    match = re.fullmatch(r'(?:a |the )?number\s+(plus|minus|increased by|decreased by)\s+'+number+r'\s+(?:is|equals|equal to)\s+'+number,text)
    if match:
        operation,a,b=match.groups(); sign='+' if operation in ('plus','increased by') else '-'
        return f'x{sign}{a}={b}', 'linear_word_problem', 'Let x be the unknown number.'
    match = re.search(r'(?:sum of (?:two|2) numbers|two numbers (?:have a )?sum)\s+(?:is |of )?'+number+r'.*?(?:difference (?:is |of )?)'+number,text)
    if match:
        a,b=match.groups(); return f'x+y={a};x-y={b}', 'sum_difference', 'Let x and y be the two numbers; use their sum and difference.'
    match = re.search(r'(?:rectangle|rectangular).*?length\s+(?:of |is |= )?'+number+r'.*?width\s+(?:of |is |= )?'+number,text)
    if match and 'area' in text:
        a,b=match.groups(); return f'{a}*{b}', 'rectangle_area', 'Area equals length times width; both lengths must use the same unit.'
    match = re.search(r'(?:speed|travels at|moving at)\s+(?:of |is |= )?'+number+r'\s*(km/h|mph|m/s|kilometers per hour|miles per hour|meters per second).*?(?:for|time)\s+'+number+r'\s*(hours?|minutes?|seconds?)',text)
    if match and any(word in text for word in ('distance','how far')):
        speed,unit,duration,time_unit=match.groups()
        factor = '1' if (unit in ('m/s','meters per second') and time_unit.startswith('second')) or (unit not in ('m/s','meters per second') and time_unit.startswith('hour')) else (
            '60' if unit in ('m/s','meters per second') and time_unit.startswith('minute') else
            '3600' if unit in ('m/s','meters per second') else '1/60' if time_unit.startswith('minute') else '1/3600')
        return f'{speed}*{duration}*({factor})', 'distance', 'Distance equals speed times time. Result units follow the distance unit in the speed.'
    raise MathInputError('This wording is not supported yet. Enter the equation directly, or use a percentage, number, sum/difference, rectangle-area or speed/time problem.')

def parse_calculus(source, operation, variable):
    bounds = None
    if source.startswith('\\int'):
        operation = 'integrate'
        source = source[4:].strip()
        match = re.match(r'_\{([^{}]+)\}\^\{([^{}]+)\}', source)
        if match:
            bounds=match.groups(); source=source[match.end():].strip()
        match = re.search(r'(?:\\,|\s)*d\s*([A-Za-z])\s*$',source)
        if not match:
            raise MathInputError('End the integral with its differential, for example dx.')
        variable=match.group(1); source=source[:match.start()].strip()
    match = re.fullmatch(r'\\frac\{d\}\{d([A-Za-z])\}\s*(.*)',source)
    if match:
        operation='differentiate'; variable=match.group(1); source=match.group(2)
    return source,operation,variable,bounds

def build_bindings(values, compiler):
    if len(values)>64:
        raise MathInputError('Use at most 64 page variables.')
    trees = {}
    for name,value in values.items():
        if not re.fullmatch(r'[A-Za-z][A-Za-z0-9_]{0,31}',name) or name in ('e','i','pi','oo'):
            raise MathInputError(f'Invalid or reserved variable name: {name}')
        trees[name]=parse(value,values)
    resolved,visiting={},set()
    def resolve(name):
        if name in resolved: return resolved[name]
        if name in visiting: raise MathInputError('Page variable definitions contain a cycle.')
        visiting.add(name)
        def dependencies(node):
            found={node.value} if node.kind=='symbol' else set()
            for child in node.children: found.update(dependencies(child))
            return found
        for dependency in dependencies(trees[name]):
            if dependency in trees: resolve(dependency)
            elif dependency not in ('pi','e','i','oo'):
                raise MathInputError(f'Undefined variable {dependency} in the definition of {name}.')
        compiler.bindings=resolved
        result=normal(compiler.compile(trees[name]))
        if result.free_symbols or result.has(s.zoo,s.nan,s.oo,-s.oo):
            raise MathInputError(f'Variable {name} is not a finite defined value.')
        resolved[name]=result; visiting.remove(name); return result
    for name in trees: resolve(name)
    compiler.bindings=resolved
    return resolved

def sample_graph(expression, variable, x_min, x_max, compiler):
    if not math.isfinite(x_min) or not math.isfinite(x_max) or x_min>=x_max or abs(x_min)>1e6 or abs(x_max)>1e6:
        raise MathInputError('Graph range must be finite, increasing and within ±1000000.')
    if expression.free_symbols-{variable}:
        raise MathInputError('Define the parameters before graphing this function.')
    segments, current=[],[]
    def value(x):
        try:
            result=expression.subs(variable,s.Float(x)).evalf(15)
            if result.is_real is not True: return None
            result=float(result)
            if not math.isfinite(result) or abs(result)>1e8: return None
            if check_conditions(s.Float(x),variable,compiler.conditions) is not True: return None
            return result
        except (ValueError,TypeError,OverflowError,ZeroDivisionError): return None
    # Midpoint subdivision rejects intervals containing gaps, poles or sharp
    # nonlinearity instead of joining points blindly across discontinuities.
    samples=[]
    def interval(a,fa,b,fb,depth):
        midpoint=(a+b)/2; fm=value(midpoint)
        discontinuity=fa is None or fb is None or fm is None
        curvature=not discontinuity and abs(fm-(fa+fb)/2)>0.025*(1+abs(fm))
        if depth<5 and (discontinuity or curvature):
            interval(a,fa,midpoint,fm,depth+1); interval(midpoint,fm,b,fb,depth+1)
        elif discontinuity or curvature:
            samples.append((midpoint,None)); samples.append((b,fb))
        else:
            samples.append((b,fb))
    count=80
    left=x_min; previous=value(left); samples.append((left,previous))
    for index in range(1,count+1):
        right=x_min+(x_max-x_min)*index/count; result=value(right)
        interval(left,previous,right,result,0); left,previous=right,result
    for x,y in samples:
        if y is None:
            if len(current)>1: segments.append(current)
            current=[]
        else: current.append([x,y])
    if len(current)>1: segments.append(current)
    if not segments: raise MathInputError('The function has no finite real values in this graph range.')
    ys=sorted(point[1] for segment in segments for point in segment)
    # A percentile viewport keeps a pole from flattening the useful curve.
    low,high=ys[int(len(ys)*.03)],ys[min(len(ys)-1,int(len(ys)*.97))]
    if high-low<1e-8: low-=1; high+=1
    pad=(high-low)*.12; y_min,y_max=low-pad,high+pad
    width,height=640,380
    def xy(x,y): return 44+(x-x_min)/(x_max-x_min)*576,20+(y_max-y)/(y_max-y_min)*328
    svg=[f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
         '<rect width="640" height="380" fill="white"/>',
         '<defs><clipPath id="plot"><rect x="44" y="20" width="576" height="328"/></clipPath></defs>']
    for index in range(6):
        gx=44+index*576/5; gy=20+index*328/5
        svg.extend([f'<path d="M{gx} 20V348 M44 {gy}H620" stroke="#e4e7eb" fill="none"/>',
                    f'<text x="{gx}" y="368" text-anchor="middle" font-size="11" fill="#333">{x_min+(x_max-x_min)*index/5:.3g}</text>',
                    f'<text x="39" y="{gy+4}" text-anchor="end" font-size="11" fill="#333">{y_max-(y_max-y_min)*index/5:.3g}</text>'])
    if x_min<=0<=x_max:
        gx,_=xy(0,0);svg.append(f'<path d="M{gx} 20V348" stroke="#889099"/>')
    if y_min<=0<=y_max:
        _,gy=xy(0,0);svg.append(f'<path d="M44 {gy}H620" stroke="#889099"/>')
    for segment in segments:
        path=' '.join(('M' if index==0 else 'L')+f'{xy(x,y)[0]:.3f},{xy(x,y)[1]:.3f}' for index,(x,y) in enumerate(segment))
        svg.append(f'<path d="{path}" stroke="#2463ae" stroke-width="2" fill="none" clip-path="url(#plot)"/>')
    svg.append('</svg>')
    return {'segments':segments,'x_min':x_min,'x_max':x_max,'y_min':y_min,'y_max':y_max,'svg':''.join(svg)}

def check_work(original, next_line, variable, compiler):
    other_compiler=Compiler(compiler.domain,compiler.angle,compiler.bindings)
    other=other_compiler.compile(parse(next_line,compiler.bindings))
    if isinstance(original,Equality) and isinstance(other,Equality):
        original_set=solution_set(original,variable,compiler)
        next_set=solution_set(other,variable,other_compiler)
        equality=original_set==next_set
        if not equality and s.SymmetricDifference(original_set,next_set).is_empty is None:
            return other,'unknown','Could not establish whether these equations have the same solution set.'
        if equality:
            return other,'verified','This line preserves the solution set in the selected domain.'
        lost=s.Complement(original_set,next_set); added=s.Complement(next_set,original_set)
        return other,'incorrect',f'The solution set changed. Lost solutions: {s.sstr(lost)}; added solutions: {s.sstr(added)}.'
    if isinstance(original,Relational) or isinstance(other,Relational):
        return other,'unknown','Equation and expression types must match; inequality step checking is not supported yet.'
    difference=normal(original-other)
    if difference!=0:
        equality=difference.equals(0)
        return other,'incorrect' if equality is False else 'unknown','These expressions differ.' if equality is False else 'Could not prove that these expressions are equivalent.'
    old_conditions=s.And(*compiler.conditions)
    new_conditions=s.And(*other_compiler.conditions)
    if old_conditions!=new_conditions:
        return other,'conditional','The expressions agree where the original expression is defined. Keep the original domain restrictions.'
    return other,'verified','The expressions are equivalent with the same recorded restrictions.'

def check_real_integral_domain(lower, upper, variable, conditions):
    if lower.free_symbols or upper.free_symbols or lower.is_real is False or upper.is_real is False:
        raise MathInputError('Choose real bounds with defined parameter values for this definite integral.')
    if lower == upper:
        return
    interval = s.Interval(s.Min(lower, upper), s.Max(lower, upper))
    permitted = s.S.Reals
    for condition in conditions:
        if condition.free_symbols - {variable}:
            raise MathInputError('Define the remaining parameters before checking the integral domain.')
        try:
            permitted = s.Intersection(permitted, s.solve_univariate_inequality(condition, variable, relational=False))
        except (NotImplementedError, ValueError, TypeError):
            raise MathInputError('Could not verify the original domain over this integration interval.')
    excluded = s.Complement(interval, permitted)
    # Isolated excluded endpoints/poles are handled by SymPy's convergence
    # calculation. An undefined subinterval cannot become a real integral.
    if excluded.is_empty is not True and not isinstance(excluded, s.FiniteSet):
        raise MathInputError('The integration interval extends outside the original real domain.')

def solve(request):
    source=request.get('expression','').strip()
    operation=request.get('operation','auto')
    domain=request.get('domain','real'); angle=request.get('angle','radians')
    if operation not in OPERATIONS or domain not in ('real','complex') or angle not in ('radians','degrees'):
        raise MathInputError('Unsupported operation, domain or angle setting.')
    if operation=='interpret':
        translated,kind,explanation=interpret_words(source)
        return {'version':VERSION,'input':source,'interpretation':translated,'answer':translated,
                'answer_latex':'','approximate':'','title':'Review the translated problem',
                'restrictions':[],'steps':[],'methods':[],'status':'review',
                'message':explanation,'interpretation_kind':kind}
    # Strip only known complete equation-layout wrappers, preserving equation order.
    layout=re.fullmatch(r'\\begin\{(cases|aligned|align|gathered)\}(.*?)\\end\{\1\}',source,re.S)
    if layout: source=layout[2].replace('&','')
    degree=re.compile(r'(?:\^\{\\circ\}|\^\\circ|°)')
    if degree.search(source):
        if angle!='degrees': raise MathInputError('This expression uses degree marks; choose Degrees.')
        source=degree.sub('',source)
    compiler=Compiler(domain,angle)
    if operation=='assign':
        assignment=parse(source,request.get('variables',{}))
        if assignment.kind!='relation' or assignment.value not in ('=',':=') or assignment.children[0].kind!='symbol':
            raise MathInputError('Define a variable with name := expression, for example a := 5.')
        name=assignment.children[0].value
        separator=':=' if ':=' in source else '='
        value=source.split(separator,1)[1].strip()
        values=dict(request.get('variables',{}));values[name]=value
        bindings=build_bindings(values,compiler)
        result=bindings[name]
        equation=s.Eq(compiler.symbol(name),result,evaluate=False)
        return {'version':VERSION,'input':source,'answer':str(equation),'answer_latex':latex(equation),
                'approximate':'','title':'Page variable','assignment':name,'assignment_value':value,
                'status':'verified','restrictions':[],'methods':[],'message':'This definition is available to calculations on the current page.',
                'steps':[step('define_variable',equation,equation,'Define this page variable and its dependencies.')]}
    bindings=build_bindings(request.get('variables',{}),compiler)
    compiler.conditions=[]  # Conditions from resolved numeric bindings have already been checked.
    variable_name=request.get('variable','x') or 'x'
    if not re.fullmatch(r'[A-Za-z][A-Za-z0-9_]{0,31}',variable_name) or variable_name in ('pi','e','i','oo'):
        raise MathInputError('Choose a valid target variable name.')
    source,operation,variable_name,integral_bounds=parse_calculus(source,operation,variable_name)
    compiler.bindings={name:value for name,value in bindings.items() if name!=variable_name or operation in ('evaluate','simplify','factor','live','auto')}
    lines=[line.strip() for line in re.split(r';|\n|\\\\',source) if line.strip()]
    if not lines or len(lines)>8:
        raise MathInputError('Select one problem or a system with at most eight equations.')
    trees=[parse(line,bindings) for line in lines]
    expressions=[compiler.compile(tree) for tree in trees]
    expression=expressions[0]
    variable=compiler.symbol(variable_name)
    free=set().union(*(value.free_symbols for value in expressions))
    if operation in ('auto','live'):
        operation='solve' if isinstance(expression,Relational) else 'simplify'
        if len(lines)>1: operation='solve'
    if operation in ('solve','check') and variable not in free and len(free)==1:
        variable=next(iter(free));variable_name=str(variable)
    report={'version':VERSION,'input':source,'input_latex':latex(expression),'source_tree':trees[0].json(),
            'answer':'','answer_latex':'','approximate':'','title':'Solution','restrictions':restrictions(compiler.conditions),
            'steps':[],'methods':[],'status':'verified','message':'','variable':variable_name,
            'domain':domain,'angle':angle,'operation':operation,'graph':None}
    if operation=='check':
        compare=request.get('next_line','').strip()
        if not compare: raise MathInputError('Enter the next line of your work.')
        other,status,message=check_work(expression,compare,variable,compiler)
        report.update(answer=message,answer_latex=latex(other),status=status,message=message,title='Check your work')
        report['steps']=[step('check_work',expression,other,message,status=status)]
        return report
    if operation=='graph':
        if isinstance(expression,Equality):
            if str(expression.lhs)!='y': raise MathInputError('Use an explicit graph equation, for example y = x^2.')
            expression=expression.rhs
        graph=sample_graph(expression,variable,float(request.get('x_min',-10)),float(request.get('x_max',10)),compiler)
        report.update(answer='Function graph',answer_latex=latex(expression),graph=graph,title='Graph')
        report['steps']=[step('graph',expression,expression,'Sample the function on its permitted real domain.',
                              'Gaps and steep unresolved intervals are not joined across discontinuities.')]
        return report
    if len(expressions)>1:
        if operation!='solve' or not all(isinstance(e,Equality) for e in expressions):
            raise MathInputError('Multiple lines must form an equation system.')
        variables=sorted(free,key=s.default_sort_key)
        if len(variables)>4: raise MathInputError('Use at most four unknowns in a system.')
        try: matrix,vector=s.linear_eq_to_matrix([e.lhs-e.rhs for e in expressions],variables)
        except NonlinearError: raise MathInputError('Step-by-step systems currently support linear equations.')
        augmented=matrix.row_join(vector)
        reduced,row_steps=row_reduction_steps(augmented)
        answer=s.linsolve((matrix,vector),variables)
        verified=[]
        for values in answer:
            substitutions=dict(zip(variables,values))
            if domain=='real' and any(value.is_real is False for value in values): continue
            conditions=[normal(condition.subs(substitutions)) for condition in compiler.conditions]
            if any(condition is s.S.false for condition in conditions): continue
            if any(condition is not s.S.true for condition in conditions): report['status']='conditional'
            residuals=[normal((equation.lhs-equation.rhs).subs(substitutions)) for equation in expressions]
            if any(residual.is_zero is False for residual in residuals): continue
            if any(residual != 0 for residual in residuals):
                raise MathInputError('Could not verify this system solution in the original equations.')
            verified.append(values)
        answer=s.FiniteSet(*verified)
        report['steps']=[step('augmented_matrix',augmented,augmented,'Write the system as an augmented matrix.')]+row_steps+[
                         step('read_solution',reduced,answer,'Read the solution, retaining any free parameters.')]
        report['variables']=[str(v) for v in variables]
        report['methods']=['elimination']
    elif operation=='solve':
        if not isinstance(expression,Relational): expression=s.Eq(expression,0,evaluate=False)
        if not isinstance(expression,Equality):
            if domain!='real' or free-{variable}: raise MathInputError('Inequalities require one real unknown.')
            answer,report['steps']=inequality_steps(expression,variable,compiler)
            report['methods']=['intervals']
        else:
            if free-{variable}: raise MathInputError('Define the other variables or select a system of equations.')
            answer,report['steps'],report['methods']=equation_steps(expression,variable,compiler,request.get('method',''))
    elif operation=='differentiate':
        if isinstance(expression,Relational): raise MathInputError('Differentiate an expression, not an equation.')
        answer=s.diff(expression,variable)
        denominator=s.denom(s.together(answer))
        if denominator != 1: compiler.condition(s.Ne(denominator,0))
        for absolute in expression.atoms(s.Abs):
            if absolute.args[0].has(variable): compiler.condition(s.Ne(absolute.args[0],0))
        report['steps']=derivative_steps(expression,variable)
        report['methods']=['rules']
    elif operation=='integrate':
        if isinstance(expression,Relational): raise MathInputError('Integrate an expression, not an equation.')
        answer,report['steps']=integration_steps(expression,variable,domain)
        report['methods']=['manual_integration']
        if integral_bounds:
            lower=normal(compiler.compile(parse(integral_bounds[0],bindings)))
            upper=normal(compiler.compile(parse(integral_bounds[1],bindings)))
            if domain == 'real':
                check_real_integral_domain(lower, upper, variable, compiler.conditions)
            definite=s.integrate(expression,(variable,lower,upper))
            if definite.has(s.Integral,s.zoo,s.nan,s.oo,-s.oo) or (domain == 'real' and definite.is_real is False): raise MathInputError('Could not establish a finite definite integral.')
            report['steps'].append(step('definite_integral',s.Integral(expression,(variable,lower,upper)),definite,
                                        'Evaluate the definite integral with its interval and convergence conditions.'))
            answer=definite
    else:
        if isinstance(expression,Relational): raise MathInputError('Choose Solve for an equation or inequality.')
        answer,report['steps']=simplify_steps(expression,operation)
        if operation=='evaluate' and answer.free_symbols: raise MathInputError('Define the variables before requesting a numeric answer.')
        if answer.has(s.zoo,s.nan): raise MathInputError('This expression is undefined.')
    report['restrictions']=restrictions(compiler.conditions)
    report['answer']=s.sstr(answer)
    report['answer_latex']=latex(answer)
    if operation=='solve' and isinstance(answer,s.Set):
        unknowns = s.Tuple(*variables) if len(expressions)>1 else variable
        if isinstance(answer,s.FiniteSet) and len(answer)==1:
            report['answer_latex']=latex(unknowns)+' = '+latex(next(iter(answer)))
        else:
            report['answer_latex']=latex(unknowns)+r' \in '+latex(answer)
    if not answer.free_symbols and not isinstance(answer,(s.Set,Relational,s.Integer)):
        approximate=str(answer.evalf(12))
        approximate=re.sub(r'(\.\d*?[1-9])0+(?=[eE]|$|\s|\*)',r'\1',approximate)
        report['approximate']=re.sub(r'\.0+(?=[eE]|$|\s|\*)','',approximate)
    if any(item['status']=='summary' for item in report['steps']):
        report['message']='Some operations are summarized; a detailed teaching trace is not available for every problem type.'
    return report

@lru_cache(maxsize=64)
def cached_solve(serialized):
    return solve(json.loads(serialized))

def timeout(*_):
    raise TimeoutError('This problem exceeded the solver time limit; try a smaller expression.')

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--config',type=Path,required=True)
    args=parser.parse_args()
    config=json.loads(args.config.read_text())
    seconds=max(1,min(20,int(config.get('timeout_seconds',8))))
    # This service is launched independently of Torch; bound process memory too.
    resource.setrlimit(resource.RLIMIT_AS,(768*1024*1024,768*1024*1024))
    if sys.platform=='linux':
        import ctypes
        parent=os.getppid();ctypes.CDLL(None).prctl(1,signal.SIGTERM,0,0,0)
        if os.getppid()!=parent: return
    signal.signal(signal.SIGALRM,timeout)
    for line in sys.stdin:
        try:
            if len(line)>65536: raise MathInputError('Solver request is too large.')
            request=json.loads(line)
            if not isinstance(request,dict): raise MathInputError('Invalid solver request.')
            signal.setitimer(signal.ITIMER_REAL,seconds)
            response=cached_solve(json.dumps(request,sort_keys=True))
        except (Exception,MemoryError) as error:
            response={'error':str(error) or type(error).__name__}
        finally:
            signal.setitimer(signal.ITIMER_REAL,0)
        print(json.dumps(response,ensure_ascii=False,allow_nan=False),flush=True)

if __name__=='__main__': main()
