"""Restricted, non-evaluating math parser. Source trees retain domain restrictions.

No Python string evaluation, unrestricted sympification, or external TeX commands.
"""
from dataclasses import dataclass, field
import re

class MathInputError(ValueError):
    pass

@dataclass
class Node:
    kind: str
    value: str = ''
    children: list = field(default_factory=list)
    start: int = 0
    end: int = 0

    def json(self):
        return {'kind': self.kind, 'value': self.value,
                'children': [child.json() for child in self.children],
                'span': [self.start, self.end]}

FUNCTIONS = {'sin', 'cos', 'tan', 'cot', 'sec', 'csc', 'asin', 'acos', 'atan',
             'sinh', 'cosh', 'tanh', 'log', 'ln', 'exp', 'sqrt', 'abs'}
COMMANDS = {'frac', 'dfrac', 'tfrac', 'sqrt', 'left', 'right', 'cdot', 'times',
            'div', 'pi', 'infty', 'le', 'leq', 'ge', 'geq', 'neq', 'ne',
            'mathrm', 'operatorname', 'text', 'degree', 'circ', 'vert', 'lvert', 'rvert'} | FUNCTIONS
TOKEN = re.compile(r'\\[A-Za-z]+|\\[,;! ]|\d+(?:\.\d*)?(?:[eE][+-]?\d+)?|'
                   r'[A-Za-z][A-Za-z0-9]*|<=|>=|!=|:=|\*\*|[^\s]')
ALIASES = {'×': '*', '÷': '/', '−': '-', 'π': 'pi', '∞': 'oo', '≤': '<=', '≥': '>=',
           '\\cdot': '*', '\\times': '*', '\\div': '/', '\\pi': 'pi', '\\infty': 'oo',
           '\\le': '<=', '\\leq': '<=', '\\ge': '>=', '\\geq': '>=',
           '\\ne': '!=', '\\neq': '!=', '**': '^', '?': 'unknown', '\\vert': '|',
           '\\lvert': '|', '\\rvert': '|'}

class Parser:
    def __init__(self, source, names=()):
        if not isinstance(source, str) or not source.strip() or len(source) > 8192:
            raise MathInputError('Enter a mathematical expression of 1–8192 characters.')
        self.source, self.names = source, set(names)
        self.tokens = []
        for match in TOKEN.finditer(source):
            token = match.group()
            if token in ('\\left', '\\right', '\\,', '\\;', '\\!', '\\ '):
                continue
            if token.startswith('\\') and token[1:] not in COMMANDS:
                raise MathInputError(f'Unsupported math command: {token}')
            if token in ('$', '`', '"', "'", ':', '@', '#'):
                raise MathInputError(f'Unsupported character: {token}')
            self.tokens.append((ALIASES.get(token, token), match.start(), match.end()))
        if len(self.tokens) > 2048:
            raise MathInputError('Expression is too large; select a smaller problem.')
        self.position = self.depth = 0

    def peek(self):
        return self.tokens[self.position][0] if self.position < len(self.tokens) else ''

    def take(self, expected=None):
        if self.position >= len(self.tokens):
            raise MathInputError('Incomplete expression.')
        token = self.tokens[self.position]
        if expected is not None and token[0] != expected:
            raise MathInputError(f'Expected {expected}, found {token[0]}.')
        self.position += 1
        return token

    def node(self, kind, value='', children=(), start=0, end=None):
        if end is None:
            end = self.tokens[self.position-1][2] if self.position else start
        return Node(kind, value, list(children), start, end)

    def group(self):
        opening = self.take()[0]
        closing = {'(': ')', '{': '}', '[': ']'}[opening]
        child = self.expression()
        self.take(closing)
        return child

    def atom(self):
        self.depth += 1
        if self.depth > 80:
            raise MathInputError('Expression is nested too deeply.')
        try:
            token, start, end = self.take()
            if token in ('+', '-'):
                child = self.expression(25)
                return self.node('neg' if token == '-' else 'positive', children=[child], start=start)
            if token in ('(', '{', '['):
                self.position -= 1
                child = self.group()
                return self.node('group', children=[child], start=start)
            if token == '|':
                child = self.expression(0, stop_bar=True)
                self.take('|')
                return self.node('function', 'abs', [child], start)
            if re.fullmatch(r'\d+(?:\.\d*)?(?:[eE][+-]?\d+)?', token):
                exponent = re.search(r'[eE]([+-]?\d+)$', token)
                if exponent and abs(int(exponent[1])) > 1000:
                    raise MathInputError('Scientific exponent exceeds 1000.')
                if len(token) > 100:
                    raise MathInputError('Number is too long.')
                return self.node('number', token, start=start, end=end)
            if token in ('\\frac', '\\dfrac', '\\tfrac'):
                numerator = self.required_group()
                denominator = self.required_group()
                return self.node('div', children=[numerator, denominator], start=start)
            if token == '\\sqrt':
                index = self.group() if self.peek() == '[' else None
                child = self.required_group() if self.peek() == '{' else self.atom()
                return self.node('root', children=[child, index] if index else [child], start=start)
            if token in ('\\mathrm', '\\operatorname', '\\text'):
                self.take('{')
                parts = []
                while self.peek() and self.peek() != '}':
                    parts.append(self.take()[0])
                self.take('}')
                name = ''.join(parts)
                if not re.fullmatch(r'[A-Za-z][A-Za-z0-9]*', name):
                    raise MathInputError('Only a variable or function name is allowed inside this command.')
                token = name
            token = token.lstrip('\\')
            if token in FUNCTIONS:
                exponent = None
                if self.peek() == '^':
                    self.take()
                    exponent = self.atom()
                base = None
                if self.peek() == '_':
                    self.take()
                    base = self.atom()
                if self.peek() in ('(', '{', '['):
                    child = self.group()
                else:
                    child = self.expression(21)
                inverse = exponent
                while inverse and inverse.kind == 'group':
                    inverse = inverse.children[0]
                if token in ('sin', 'cos', 'tan') and inverse and inverse.kind == 'neg' and inverse.children[0].value == '1':
                    token = 'a' + token
                    exponent = None
                node = self.node('function', token, [child] + ([base] if base else []), start)
                return self.node('pow', children=[node, exponent], start=start) if exponent else node
            if not re.fullmatch(r'[A-Za-z][A-Za-z0-9]*', token):
                raise MathInputError(f'Expected a number or variable, found {token}.')
            if self.peek() == '_':
                self.take()
                subscript = self.group() if self.peek() == '{' else self.atom()
                while subscript.kind == 'group': subscript = subscript.children[0]
                if subscript.kind not in ('number','symbol') or not re.fullmatch(r'[A-Za-z0-9]+',subscript.value):
                    raise MathInputError('Variable subscripts must be a simple name or nonnegative integer.')
                token += '_' + subscript.value
                end = subscript.end
            if self.peek() == '(' and token not in self.names:
                raise MathInputError(f'{token}(...) is ambiguous. Use {token} * (...) for multiplication or a supported function.')
            return self.node('symbol', token, start=start, end=end)
        finally:
            self.depth -= 1

    def required_group(self):
        if self.peek() != '{':
            raise MathInputError('This LaTeX command needs braces around each argument.')
        return self.group()

    def expression(self, minimum=0, stop_bar=False):
        left = self.atom()
        while True:
            token = self.peek()
            if not token or token in (')', '}', ']', '=', '<', '>', '<=', '>=', '!=', ':=', ';', ',') or (stop_bar and token == '|'):
                break
            if token == '!':
                self.take()
                left = self.node('function', 'factorial', [left], left.start)
                continue
            if token == '%':
                self.take()
                left = self.node('div', children=[left, Node('number', '100')], start=left.start)
                continue
            if token == '_':
                self.take()
                index = self.atom()
                if left.kind != 'symbol' or index.kind not in ('symbol', 'number', 'group'):
                    raise MathInputError('Only variable subscripts are supported.')
                value = index.children[0].value if index.kind == 'group' else index.value
                left = self.node('symbol', left.value + '_' + value, start=left.start)
                continue
            explicit = token in ('+', '-', '*', '/', '^')
            precedence = {'+': 10, '-': 10, '*': 20, '/': 20, '^': 30}.get(token, 20)
            if precedence < minimum:
                break
            if not explicit and not (token in ('(', '{', '[', '|') or re.match(r'[A-Za-z0-9\\]', token)):
                raise MathInputError(f'Unexpected symbol: {token}')
            number = left
            while number.kind in ('neg','positive'): number = number.children[0]
            if not explicit and number.kind == 'number' and re.fullmatch(r'\d+(?:\.\d*)?(?:[eE][+-]?\d+)?',token):
                raise MathInputError('Adjacent numbers are ambiguous. Join the digits or write an explicit multiplication sign.')
            if explicit:
                self.take()
            right = self.expression(precedence if token == '^' else precedence+1, stop_bar)
            kind = {'+': 'add', '-': 'sub', '*': 'mul', '/': 'div', '^': 'pow'}.get(token, 'mul')
            left = self.node(kind, children=[left, right], start=left.start)
        return left

    def parse(self):
        left = self.expression()
        if self.peek() in ('=', '<', '>', '<=', '>=', '!=', ':='):
            relation = self.take()[0]
            if not self.peek() and relation == '=':
                return left  # Calculator-style completed equals sign.
            right = self.expression()
            left = self.node('relation', relation, [left, right], left.start)
        if self.peek():
            raise MathInputError(f'Unexpected trailing input: {self.peek()}')
        return left

def parse(source, names=()):
    for left, right in [('\\[', '\\]'), ('\\(', '\\)'), ('$$', '$$'), ('$', '$')]:
        if source.startswith(left) and source.endswith(right):
            source = source[len(left):-len(right)].strip()
            break
    return Parser(source, names).parse()

class Compiler:
    def __init__(self, domain='real', angle='radians', bindings=None):
        import sympy
        self.s = sympy
        self.domain, self.angle = domain, angle
        self.bindings = bindings or {}
        self.conditions, self.symbols = [], {}

    def symbol(self, name):
        if name not in self.symbols:
            self.symbols[name] = self.s.Symbol(name, real=True) if self.domain == 'real' else self.s.Symbol(name)
        return self.symbols[name]

    def condition(self, condition):
        if condition is self.s.S.false:
            raise MathInputError('Expression is undefined under the selected assumptions.')
        if condition is not self.s.S.true and condition not in self.conditions:
            self.conditions.append(condition)

    def compile(self, node):
        s = self.s
        kind = node.kind
        if kind == 'number':
            return s.Rational(node.value)
        if kind == 'symbol':
            if node.value in ('pi', 'e', 'oo', 'i'):
                return {'pi': s.pi, 'e': s.E, 'oo': s.oo, 'i': s.I}[node.value]
            return self.bindings.get(node.value, self.symbol(node.value))
        args = [self.compile(child) for child in node.children]
        if kind in ('group', 'positive'):
            return args[0]
        if kind == 'neg':
            return s.Mul(-1, args[0], evaluate=False)
        if kind == 'add':
            return s.Add(*args, evaluate=False)
        if kind == 'sub':
            return s.Add(args[0], s.Mul(-1, args[1], evaluate=False), evaluate=False)
        if kind == 'mul':
            return s.Mul(*args, evaluate=False)
        if kind == 'div':
            self.condition(s.Ne(args[1], 0))
            return s.Mul(args[0], s.Pow(args[1], -1, evaluate=False), evaluate=False)
        if kind in ('pow', 'root'):
            if kind == 'root':
                exponent = s.Pow(args[1], -1) if len(args) == 2 else s.Rational(1, 2)
            else:
                exponent = s.simplify(args[1])
            if kind == 'root' and len(args) == 2 and (args[1].is_integer is not True or args[1] < 2 or args[1] > 100):
                raise MathInputError('Root indices must be integers from 2 to 100.')
            if kind == 'root' and self.domain == 'real' and exponent.is_Rational and exponent.q % 2 == 1:
                return s.real_root(args[0], exponent.q)
            if exponent.is_number and abs(exponent) > 1000:
                raise MathInputError('Exponent is too large for an interactive calculation.')
            if exponent.is_negative:
                self.condition(s.Ne(args[0], 0))
            if self.domain == 'real' and exponent.is_Rational and exponent.q > 1 and exponent.q % 2 == 1:
                return s.real_root(args[0], exponent.q) ** exponent.p
            if self.domain == 'real' and exponent.is_Rational and exponent.q % 2 == 0:
                self.condition(s.Ge(args[0], 0))
            return s.Pow(args[0], exponent, evaluate=False)
        if kind == 'function':
            name = node.value
            argument = args[0]
            if name == 'factorial' and (argument.is_number is not True or argument>1000 or argument.is_integer is not True or argument<0):
                raise MathInputError('Interactive factorials require an integer between 0 and 1000.')
            if name in ('log','ln'):
                self.condition(s.Ne(argument,0))
                if len(args)>1: self.condition(s.Ne(args[1],0)); self.condition(s.Ne(args[1],1))
            if name in ('log', 'ln') and self.domain == 'real':
                self.condition(s.Gt(argument, 0))
                if len(args) > 1:
                    self.condition(s.Gt(args[1], 0)); self.condition(s.Ne(args[1], 1))
            if name in ('sqrt', 'abs'):
                if name == 'sqrt':
                    if self.domain == 'real': self.condition(s.Ge(argument, 0))
                    return s.Pow(argument, s.Rational(1, 2), evaluate=False)
                return s.Abs(argument, evaluate=False)
            if name in ('sin', 'cos', 'tan', 'cot', 'sec', 'csc') and self.angle == 'degrees':
                argument = argument*s.pi/180
            if name in ('tan', 'sec'):
                self.condition(s.Ne(s.cos(argument), 0))
            if name in ('cot', 'csc'):
                self.condition(s.Ne(s.sin(argument), 0))
            if name in ('asin', 'acos') and self.domain == 'real':
                self.condition(s.Ge(argument, -1)); self.condition(s.Le(argument, 1))
            if name == 'log' and len(args) == 1:
                return s.log(argument, 10, evaluate=False)
            function = getattr(s, {'ln': 'log', 'factorial': 'factorial'}.get(name, name))
            value = function(argument, *args[1:], evaluate=False)
            return value*180/s.pi if name in ('asin', 'acos', 'atan') and self.angle == 'degrees' else value
        if kind == 'relation':
            functions = {'=': s.Eq, ':=': s.Eq, '!=': s.Ne, '<': s.Lt, '>': s.Gt, '<=': s.Le, '>=': s.Ge}
            return functions[node.value](*args, evaluate=False)
        raise MathInputError(f'Unsupported mathematical structure: {kind}')
