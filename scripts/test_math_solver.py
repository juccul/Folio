"""Meaningful mathematical, parsing, domain and explanation regressions."""
import importlib.util
from pathlib import Path
import socket
import unittest

spec=importlib.util.spec_from_file_location('solver',Path(__file__).with_name('math-solver-worker.py'))
solver=importlib.util.module_from_spec(spec);spec.loader.exec_module(solver)

def solve(expression,operation='auto',**options):
    return solver.solve(dict(expression=expression,operation=operation,**options))

class SolverTests(unittest.TestCase):
    def test_spaced_digits_require_review_instead_of_silent_multiplication(self):
        with self.assertRaisesRegex(ValueError,'Adjacent numbers'):
            solve('2 x+3=1 1')
        self.assertEqual(solve('1*1=')['answer'],'1')
        self.assertEqual(solve('2 x+3=11')['answer'],'{4}')

    def test_real_fractional_power_and_derivative_boundary_domains(self):
        self.assertEqual(solve('(-8)^(1/3)')['answer'],'-2')
        self.assertEqual(solve('(-8)^(-1/3)')['answer'],'-1/2')
        result=solve('sqrt(x)',operation='differentiate')
        self.assertTrue(any('neq' in condition for condition in result['restrictions']))
        result=solve('abs(x)',operation='differentiate')
        self.assertIn(r'x \neq 0',result['restrictions'])
        with self.assertRaisesRegex(ValueError,'undefined'):
            solve('ln(0)',domain='complex')
        with self.assertRaisesRegex(ValueError,'factorials'):
            solve('x!')

    def test_exact_fraction_and_completed_equals(self):
        result=solve(r'\frac{1}{2}+\frac{1}{3}=')
        self.assertEqual(result['answer'],'5/6')
        self.assertTrue(result['steps'])
        self.assertEqual(solve('4/2')['approximate'],'')
        self.assertEqual(solve('1/2')['approximate'],'0.5')

    def test_restrictions_survive_cancellation(self):
        result=solve(r'\frac{x}{x}')
        self.assertEqual(result['answer'],'1')
        self.assertIn(r'x \neq 0',result['restrictions'])
        self.assertEqual(result['source_tree']['kind'],'div')

    def test_linear_steps_and_roots(self):
        result=solve('2x+3=11')
        self.assertEqual(result['answer'],'{4}')
        self.assertEqual(result['answer_latex'],'x = 4')
        self.assertTrue(any(step['rule']=='divide_coefficient' for step in result['steps']))
        self.assertTrue(any(step['rule']=='substitute_check' for step in result['steps']))

    def test_quadratic_methods_and_domains(self):
        for method in ('factor','quadratic_formula','complete_square'):
            result=solve('x^2-5x+6=0',method=method)
            self.assertEqual(result['answer'],'{2, 3}')
            self.assertIn(method,result['methods'])
        self.assertEqual(solve('x^2+1=0')['answer'],'EmptySet')
        self.assertIn('I',solve('x^2+1=0',domain='complex')['answer'])

    def test_extraneous_and_denominator_roots(self):
        self.assertEqual(solve(r'\sqrt{x+6}=x')['answer'],'{3}')
        self.assertEqual(solve('x/(x-1)=0')['answer'],'{0}')
        self.assertIn('Abs',solve(r'\sqrt{x^2}')['answer'])

    def test_linear_system_and_inequality(self):
        self.assertEqual(solve('x+y=5;2x-y=1')['answer'],'{(2, 3)}')
        self.assertIn('2',solve('x^2>4',operation='solve')['answer'])
        self.assertEqual(solve('x=i;y=1')['answer'],'EmptySet')
        self.assertEqual(solve('x=i;y=1',domain='complex')['answer'],'{(I, 1)}')

    def test_derivative_and_verified_manual_integral(self):
        result=solve('sin(x^2)',operation='differentiate')
        self.assertEqual(result['answer'],'2*x*cos(x**2)')
        self.assertTrue(result['steps'][0]['children'])
        result=solve('x^2',operation='integrate')
        self.assertIn('C',result['answer'])
        self.assertTrue(any(item['rule']=='check_antiderivative' for item in result['steps']))
        self.assertEqual(solve(r'\int_{0}^{1} x^2 dx')['answer'],'1/3')

    def test_ocr_layout_subscripts_degrees_and_real_branches(self):
        self.assertEqual(solve(r'\begin{cases} x+y&=5\\2x-y&=1\end{cases}')['answer'],'{(2, 3)}')
        self.assertEqual(solve(r'x_{1}+2=5',variable='x_1')['answer'],'{3}')
        self.assertEqual(solve(r'\sin(30^{\circ})',angle='degrees')['answer'],'1/2')
        with self.assertRaisesRegex(ValueError,'Degrees'):
            solve(r'\sin(30^{\circ})')
        self.assertEqual(solve(r'\sqrt[3]{-8}')['answer'],'-2')
        self.assertIn('log(Abs(x))',solve('1/x',operation='integrate')['answer'])

    def test_page_variables_dependencies_and_cycles(self):
        self.assertEqual(solve('a+b=',variables={'a':'2','b':'3*a'})['answer'],'8')
        with self.assertRaisesRegex(ValueError,'cycle'):
            solve('a+b',variables={'a':'b+1','b':'a+1'})
        with self.assertRaisesRegex(ValueError,'Undefined'):
            solve('a',variables={'a':'b+1'})

    def test_assignment_sign_steps_and_calculus_failures(self):
        result=solve('a:=5',operation='assign')
        self.assertEqual(result['assignment'],'a')
        self.assertEqual(result['assignment_value'],'5')
        result=solve('-2x+3<7')
        self.assertIn('reverse', ' '.join(step['explanation'] for step in result['steps']))
        result=solve('x+y=5;2x-y=1')
        self.assertTrue(any(step['rule']=='eliminate_entry' for step in result['steps']))
        with self.assertRaisesRegex(ValueError,'finite'):
            solve(r'\int_{-1}^{1} 1/x dx')
        self.assertEqual(solve(r'\sin^{-1}(1)',angle='degrees')['answer'],'90')
        with self.assertRaisesRegex(ValueError,'exponent'):
            solve('1e999999')

    def test_degree_mode(self):
        self.assertEqual(solve('sin(30)',angle='degrees')['answer'],'1/2')
        self.assertEqual(solve('cos(pi)',angle='radians')['answer'],'-1')

    def test_check_alternative_step_and_lost_solution(self):
        self.assertEqual(solve('2x+3=11',operation='check',next_line='2x=8')['status'],'verified')
        result=solve('x*(x-1)=0',operation='check',next_line='x-1=0')
        self.assertEqual(result['status'],'incorrect')
        self.assertIn('Lost solutions',result['message'])
        self.assertEqual(solve('x/x',operation='check',next_line='1')['status'],'conditional')

    def test_word_problem_translation_is_explicit(self):
        self.assertEqual(solve('What is 15% of 200?',operation='interpret')['interpretation'],'15/100*200')
        self.assertEqual(solve('Twice a number plus 3 is 11',operation='interpret')['interpretation'],'(2)*x+3=11')
        with self.assertRaisesRegex(ValueError,'wording'):
            solve('Prove every even number is the sum of two primes.',operation='interpret')

    def test_graph_does_not_connect_across_pole(self):
        result=solve('y=1/x',operation='graph',x_min=-2,x_max=2)
        segments=result['graph']['segments']
        self.assertGreaterEqual(len(segments),2)
        self.assertFalse(any(segment[0][0]<0<segment[-1][0] for segment in segments))
        self.assertIn('<svg',result['graph']['svg'])

    def test_malformed_and_executable_input_is_rejected(self):
        for expression in ('x -','x + 2 @ garbage',r'\input{secret}',"__import__('os')",'f(x)','1/0','2^100000'):
            with self.subTest(expression=expression):
                with self.assertRaises((ValueError,TypeError)):
                    solve(expression)

    def test_network_blocked(self):
        with self.assertRaises(OSError): socket.socket(socket.AF_INET)

if __name__=='__main__': unittest.main()
