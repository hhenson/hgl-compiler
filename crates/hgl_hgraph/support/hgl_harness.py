"""The eval harness of a generated test, for the hgraph target.

Written by the hgl compiler beside the modules it generates.
"""

from hgraph.test import eval_node


def evaluate(target, **arguments):
    """Runs `target` with a dense trace for each temporal input.

    A trace is a list with one cell for each engine step from the run's
    start, and `None` where nothing is published. The result is the dense
    trace of what the target published: it spans the run's start through
    the last publication, and is empty when there was none (the language's
    TST-6).
    """
    result = eval_node(target, **arguments)
    if result is None:
        return []
    result = list(result)
    while result and result[-1] is None:
        result.pop()
    return result
